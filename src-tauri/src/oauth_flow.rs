//! Loopback-redirect OAuth flows for the subscription providers (Grok
//! SuperGrok, Google Antigravity).
//!
//! Both providers authorize through their official OAuth frontends with PKCE
//! S256 and a fixed loopback redirect — the port is part of the client
//! registration (Grok `127.0.0.1:56121`, Antigravity `localhost:8085`), so a
//! temporary local listener catches the authorization code without any
//! third-party relay. The client ids/secrets below are the public
//! "installed application" constants shipped by the reference implementation
//! (sub2api), not user secrets.
//!
//! Tokens live in the DPAPI vault exactly like every other credential; access
//! tokens expire and are refreshed through [`refresh`] right before a sync.

use std::sync::Mutex;

use base64::Engine as _;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

/// How long `complete` waits for the browser callback before giving up.
const CALLBACK_TIMEOUT_SECS: u64 = 180;
/// Refresh-window safety margin: an access token with less remaining time
/// than this is refreshed before the next upstream call.
const REFRESH_MARGIN_MS: i64 = 5 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthProvider {
    Grok,
    Antigravity,
}

impl OAuthProvider {
    pub fn from_id(value: &str) -> Option<Self> {
        match value {
            "grok" => Some(Self::Grok),
            "antigravity" => Some(Self::Antigravity),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Grok => "grok",
            Self::Antigravity => "antigravity",
        }
    }

    fn config(self) -> &'static OAuthConfig {
        match self {
            Self::Grok => &GROK_CONFIG,
            Self::Antigravity => &ANTIGRAVITY_CONFIG,
        }
    }
}

struct OAuthConfig {
    authorize_url: &'static str,
    token_url: &'static str,
    client_id: &'static str,
    client_secret: Option<&'static str>,
    scope: &'static str,
    /// Registered redirect URI; its port is fixed by the client registration.
    redirect_uri: &'static str,
    /// The loopback port to bind for the callback.
    loopback_port: u16,
    /// Extra authorize-query pairs the provider registration expects.
    authorize_extras: &'static [(&'static str, &'static str)],
}

static GROK_CONFIG: OAuthConfig = OAuthConfig {
    authorize_url: "https://auth.x.ai/oauth2/authorize",
    token_url: "https://auth.x.ai/oauth2/token",
    client_id: "b1a00492-073a-47ea-816f-4c329264a828",
    client_secret: None,
    scope: "openid profile email offline_access grok-cli:access api:access",
    redirect_uri: "http://127.0.0.1:56121/callback",
    loopback_port: 56121,
    // The registration expects these markers; keep them verbatim from the
    // reference client so the authorize screen behaves identically.
    authorize_extras: &[("plan", "generic"), ("referrer", "sub2api")],
};

static ANTIGRAVITY_CONFIG: OAuthConfig = OAuthConfig {
    authorize_url: "https://accounts.google.com/o/oauth2/v2/auth",
    token_url: "https://oauth2.googleapis.com/token",
    client_id: "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com",
    client_secret: Some("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"),
    scope: "https://www.googleapis.com/auth/cloud-platform \
            https://www.googleapis.com/auth/userinfo.email \
            https://www.googleapis.com/auth/userinfo.profile \
            https://www.googleapis.com/auth/cclog \
            https://www.googleapis.com/auth/experimentsandconfigs",
    redirect_uri: "http://localhost:8085/callback",
    loopback_port: 8085,
    authorize_extras: &[
        ("access_type", "offline"),
        ("prompt", "consent"),
        ("include_granted_scopes", "true"),
    ],
};

#[derive(Debug)]
pub enum OAuthFlowError {
    /// The loopback port is already taken (another authorization in flight?).
    PortBusy,
    /// No `begin` session is pending for `complete` to finish.
    NoSession,
    /// The provider reported an OAuth error (user denied, expired, …).
    Denied(String),
    /// The browser never called back within the deadline.
    Timeout,
    /// The callback request could not be parsed.
    InvalidCallback,
    /// The code-for-token exchange failed.
    ExchangeFailed,
    /// A network request failed.
    RequestFailed,
}

impl OAuthFlowError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::PortBusy => "OAUTH_PORT_BUSY",
            Self::NoSession => "OAUTH_NO_SESSION",
            Self::Denied(_) => "OAUTH_DENIED",
            Self::Timeout => "OAUTH_TIMEOUT",
            Self::InvalidCallback => "OAUTH_INVALID_CALLBACK",
            Self::ExchangeFailed => "OAUTH_EXCHANGE_FAILED",
            Self::RequestFailed => "OAUTH_REQUEST_FAILED",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::PortBusy => "授权端口被占用，请稍后重试或关闭其他授权窗口",
            Self::NoSession => "没有进行中的授权，请重新点击授权",
            Self::Denied(_) => "授权被拒绝或已过期，请重试",
            Self::Timeout => "等待浏览器授权超时，请重试",
            Self::InvalidCallback => "授权回调格式异常，请重试",
            Self::ExchangeFailed => "授权码交换失败，请重试",
            Self::RequestFailed => "授权网络请求失败，请检查网络后重试",
        }
    }
}

/// One pending loopback authorization. Held in [`PENDING`] between the
/// `begin` and `complete` commands. The listener is bound synchronously
/// (fail-fast on a busy port) and converted to the async listener inside
/// `complete`, where the tokio runtime is available.
struct AuthorizationSession {
    provider: OAuthProvider,
    state: String,
    verifier: String,
    listener: std::net::TcpListener,
}

static PENDING: Mutex<Option<AuthorizationSession>> = Mutex::new(None);

/// Binds the loopback listener and returns the URL to open in the system
/// browser. No network traffic happens here; the code arrives when the
/// browser redirects to the loopback port and `complete` is waiting.
pub fn begin(provider: OAuthProvider) -> Result<String, OAuthFlowError> {
    let config = provider.config();
    let mut pending = PENDING.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // Drop a stale session before binding so a re-clicked authorization can
    // reuse its port immediately (Windows refuses the rebind otherwise).
    pending.take();
    let listener = std::net::TcpListener::bind(("127.0.0.1", config.loopback_port))
        .and_then(|listener| {
            listener.set_nonblocking(true)?;
            Ok(listener)
        })
        .map_err(|_| OAuthFlowError::PortBusy)?;
    let state = random_hex(32);
    let verifier = random_urlsafe(32);
    let challenge = pkce_challenge(&verifier);

    let mut url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&nonce={}&code_challenge={}&code_challenge_method=S256",
        config.authorize_url,
        urlencode(config.client_id),
        urlencode(config.redirect_uri),
        urlencode(config.scope),
        state,
        random_hex(16),
        challenge,
    );
    for (key, value) in config.authorize_extras {
        url.push_str(&format!("&{key}={}", urlencode(value)));
    }

    *pending = Some(AuthorizationSession {
        provider,
        state,
        verifier,
        listener,
    });
    Ok(url)
}

/// The credential string persisted in the vault for OAuth providers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OAuthCredential {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
}

impl OAuthCredential {
    /// True when the access token is expired or inside the refresh margin.
    pub fn needs_refresh(&self, now_ms: i64) -> bool {
        self.access_token.is_empty() || self.expires_at_ms - now_ms <= REFRESH_MARGIN_MS
    }
}

/// Waits for the browser callback, exchanges the code for tokens, and returns
/// the ready-to-store credential JSON. Antigravity credentials also carry the
/// Cloud Companion project id when it can be resolved right away.
pub async fn complete() -> Result<(OAuthProvider, String), OAuthFlowError> {
    let session = PENDING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
        .ok_or(OAuthFlowError::NoSession)?;
    let config = session.provider.config();
    let listener = tokio::net::TcpListener::from_std(session.listener)
        .map_err(|_| OAuthFlowError::RequestFailed)?;
    let callback = wait_for_callback(listener).await?;

    if let Some(error) = callback.error {
        return Err(OAuthFlowError::Denied(error));
    }
    let code = callback.code.ok_or(OAuthFlowError::InvalidCallback)?;
    if callback.state.as_deref() != Some(session.state.as_str()) {
        return Err(OAuthFlowError::InvalidCallback);
    }

    let mut form: Vec<(&str, &str)> = vec![
        ("grant_type", "authorization_code"),
        ("client_id", config.client_id),
        ("code", &code),
        ("redirect_uri", config.redirect_uri),
        ("code_verifier", &session.verifier),
    ];
    if let Some(secret) = config.client_secret {
        form.push(("client_secret", secret));
    }
    let tokens = exchange(config.token_url, &form).await?;
    let mut credential =
        credential_from_tokens(session.provider, &tokens, None).ok_or(OAuthFlowError::ExchangeFailed)?;

    if session.provider == OAuthProvider::Antigravity {
        // Best effort: resolving the project now saves a round trip on the
        // first sync; failures leave it to the fetch path.
        if let Ok(project_id) = antigravity_project_id(&credential.access_token).await {
            credential.project_id = Some(project_id);
        }
    }
    let json = serde_json::to_string(&credential).map_err(|_| OAuthFlowError::ExchangeFailed)?;
    Ok((session.provider, json))
}

/// Refreshes an access token with the stored refresh token. Providers may
/// rotate refresh tokens; when the response omits one the previous value is
/// kept by the caller.
pub async fn refresh(
    provider: OAuthProvider,
    refresh_token: &str,
) -> Result<TokenResponse, OAuthFlowError> {
    let config = provider.config();
    let mut form: Vec<(&str, &str)> = vec![
        ("grant_type", "refresh_token"),
        ("client_id", config.client_id),
        ("refresh_token", refresh_token),
    ];
    if let Some(secret) = config.client_secret {
        form.push(("client_secret", secret));
    }
    exchange(config.token_url, &form).await
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<i64>,
    #[serde(default)]
    #[allow(dead_code)]
    pub token_type: Option<String>,
}

/// Merges a refresh response into a stored credential, keeping the old
/// refresh token when the provider did not rotate it.
pub fn refreshed_credential(
    previous: &OAuthCredential,
    tokens: &TokenResponse,
    now_ms: i64,
) -> OAuthCredential {
    OAuthCredential {
        access_token: tokens.access_token.clone(),
        refresh_token: tokens
            .refresh_token
            .clone()
            .filter(|token| !token.trim().is_empty())
            .unwrap_or_else(|| previous.refresh_token.clone()),
        expires_at_ms: now_ms + ttl_ms(tokens),
        project_id: previous.project_id.clone(),
    }
}

fn credential_from_tokens(
    provider: OAuthProvider,
    tokens: &TokenResponse,
    project_id: Option<String>,
) -> Option<OAuthCredential> {
    let margin_ms = match provider {
        // Google expires tokens at ~55 min; the reference client shaves a
        // 5-minute safety margin off the declared TTL.
        OAuthProvider::Antigravity => 300_000,
        OAuthProvider::Grok => 0,
    };
    // The offline-access scope guarantees a refresh token on the code
    // exchange; none means the provider rejected the flow quietly.
    let refresh_token = tokens
        .refresh_token
        .clone()
        .filter(|token| !token.trim().is_empty())?;
    Some(OAuthCredential {
        access_token: tokens.access_token.clone(),
        refresh_token,
        expires_at_ms: now_ms() + ttl_ms(tokens) - margin_ms,
        project_id,
    })
}

/// Token TTL in ms; a 6h default covers providers that omit `expires_in`.
fn ttl_ms(tokens: &TokenResponse) -> i64 {
    tokens
        .expires_in
        .filter(|seconds| *seconds > 0)
        .map(|seconds| seconds * 1000)
        .unwrap_or(6 * 60 * 60 * 1000)
}

async fn exchange(url: &str, form: &[(&str, &str)]) -> Result<TokenResponse, OAuthFlowError> {
    let client = http_client()?;
    let response = client
        .post(url)
        .form(form)
        .send()
        .await
        .map_err(|_| OAuthFlowError::RequestFailed)?;
    if !response.status().is_success() {
        return Err(OAuthFlowError::ExchangeFailed);
    }
    response
        .json()
        .await
        .map_err(|_| OAuthFlowError::ExchangeFailed)
}

fn http_client() -> Result<reqwest::Client, OAuthFlowError> {
    reqwest::Client::builder()
        .https_only(true)
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|_| OAuthFlowError::RequestFailed)
}

struct Callback {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// Accepts loopback connections until one carries the OAuth callback (or the
/// deadline passes). Non-callback requests (favicons, probes) are answered
/// and skipped.
async fn wait_for_callback(listener: tokio::net::TcpListener) -> Result<Callback, OAuthFlowError> {
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_secs(CALLBACK_TIMEOUT_SECS);
    loop {
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .ok_or(OAuthFlowError::Timeout)?;
        let (mut stream, _) = match tokio::time::timeout(remaining, listener.accept()).await {
            Ok(Ok(accepted)) => accepted,
            Ok(Err(_)) => return Err(OAuthFlowError::RequestFailed),
            Err(_) => return Err(OAuthFlowError::Timeout),
        };
        let mut buffer = vec![0u8; 8192];
        let read = match tokio::time::timeout(std::time::Duration::from_secs(10), async {
            stream.readable().await?;
            stream.try_read(&mut buffer)
        })
        .await
        {
            Ok(Ok(read)) => read,
            _ => continue,
        };
        let request = String::from_utf8_lossy(&buffer[..read]).to_string();
        let _ = stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n\
                  <html><body style=\"font-family:system-ui;text-align:center;padding-top:3rem\">\
                  &#10024; Authorization successful. Please return to the LLM Usage window.</body></html>",
            )
            .await;
        if let Some(callback) = parse_callback(&request) {
            return Ok(callback);
        }
        // Anything else (favicon, health probe) waits for the real callback.
    }
}

/// Extracts code/state/error from a `GET /callback?… HTTP/1.1` request line.
fn parse_callback(request: &str) -> Option<Callback> {
    let request_line = request.lines().next()?;
    let target = request_line.split_whitespace().nth(1)?;
    if !target.starts_with('/') {
        return None;
    }
    let query = target.split_once('?')?.1;
    let mut callback = Callback {
        code: None,
        state: None,
        error: None,
    };
    let mut error_description = String::new();
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=')?;
        let value = percent_decode(value);
        match key {
            "code" => callback.code = Some(value),
            "state" => callback.state = Some(value),
            "error" => callback.error = Some(value),
            "error_description" => error_description = value,
            _ => {}
        }
    }
    if let Some(error) = &mut callback.error {
        if !error_description.is_empty() {
            *error = format!("{error} ({error_description})");
        }
    }
    if callback.code.is_none() && callback.error.is_none() {
        return None;
    }
    Some(callback)
}

/// Antigravity: resolves the Cloud Companion project bound to the account.
/// Failing is fine — the fetch path retries and persists the project id.
async fn antigravity_project_id(access_token: &str) -> Result<String, OAuthFlowError> {
    let client = http_client()?;
    let response = client
        .post("https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist")
        .bearer_auth(access_token)
        .json(&serde_json::json!({
            "metadata": {"ideType": "ANTIGRAVITY", "ideVersion": "2.9.1", "ideName": "antigravity"}
        }))
        .send()
        .await
        .map_err(|_| OAuthFlowError::RequestFailed)?;
    if !response.status().is_success() {
        return Err(OAuthFlowError::RequestFailed);
    }
    let value: serde_json::Value = response.json().await.map_err(|_| OAuthFlowError::RequestFailed)?;
    value
        .get("cloudaicompanionProject")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or(OAuthFlowError::RequestFailed)
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

fn random_urlsafe(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buffer);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buffer)
}

fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buffer);
    buffer.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn urlencode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    decoded.push(byte);
                    index += 3;
                } else {
                    decoded.push(bytes[index]);
                    index += 1;
                }
            }
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_grok_authorization_url_with_pkce_and_registered_extras() {
        let url = begin(OAuthProvider::Grok).expect("bind loopback");

        let (base, query) = url.split_once('?').expect("query");
        assert_eq!(base, "https://auth.x.ai/oauth2/authorize");
        let pairs: std::collections::HashMap<&str, &str> = query
            .split('&')
            .map(|pair| pair.split_once('=').expect("key=value"))
            .collect();
        assert_eq!(pairs["response_type"], "code");
        assert_eq!(pairs["client_id"], "b1a00492-073a-47ea-816f-4c329264a828");
        assert_eq!(
            pairs["redirect_uri"],
            urlencode("http://127.0.0.1:56121/callback")
        );
        assert_eq!(pairs["code_challenge_method"], "S256");
        assert_eq!(pairs["plan"], "generic");
        // scope is url-encoded because of its spaces
        assert!(pairs.contains_key("scope"));
        assert!(pairs["state"].len() >= 32);
        assert!(pairs["code_challenge"].len() >= 43);

        // A second begin replaces the pending session (and frees the port).
        assert!(begin(OAuthProvider::Grok).is_ok());
    }

    #[test]
    fn builds_antigravity_authorization_url_with_offline_access() {
        let url = begin(OAuthProvider::Antigravity).expect("bind loopback");

        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("prompt=consent"));
        assert!(url.contains("code_challenge_method=S256"));
        // Replaces the pending session again — the slot must stay reusable.
        PENDING
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
    }

    #[test]
    fn pkce_challenge_is_the_s256_base64url_of_the_verifier() {
        // printf '%s' "test-verifier" | openssl dgst -sha256 -binary |
        //   openssl base64 | tr '+/' '-_' | tr -d '=\n'
        assert_eq!(
            pkce_challenge("test-verifier"),
            "JBbiqONGWPaAmwXk_8bT6UnlPfrn65D32eZlJS-zGG0"
        );
    }

    #[test]
    fn parses_callback_query_and_decodes_percent_escapes() {
        let callback = parse_callback(
            "GET /callback?code=abc%2F123&state=st-1 HTTP/1.1\r\nHost: 127.0.0.1:56121\r\n\r\n",
        )
        .expect("callback");
        assert_eq!(callback.code.as_deref(), Some("abc/123"));
        assert_eq!(callback.state.as_deref(), Some("st-1"));
        assert!(callback.error.is_none());
    }

    #[test]
    fn surfaces_oauth_errors_and_ignores_non_callback_requests() {
        let denied = parse_callback("GET /callback?error=access_denied HTTP/1.1").expect("denied");
        assert_eq!(denied.error.as_deref(), Some("access_denied"));

        assert!(parse_callback("GET /favicon.ico HTTP/1.1").is_none());
        assert!(parse_callback("GET /callback HTTP/1.1").is_none());
    }

    #[test]
    fn refresh_keeps_the_previous_token_when_the_response_omits_one() {
        let previous = OAuthCredential {
            access_token: "old-access".to_string(),
            refresh_token: "rotating-refresh".to_string(),
            expires_at_ms: 1_000,
            project_id: Some("project-1".to_string()),
        };
        let tokens = TokenResponse {
            access_token: "new-access".to_string(),
            refresh_token: None,
            expires_in: Some(3600),
            token_type: Some("Bearer".to_string()),
        };

        let refreshed = refreshed_credential(&previous, &tokens, 10_000);

        assert_eq!(refreshed.access_token, "new-access");
        assert_eq!(refreshed.refresh_token, "rotating-refresh");
        assert_eq!(refreshed.expires_at_ms, 10_000 + 3_600_000);
        assert_eq!(refreshed.project_id.as_deref(), Some("project-1"));
        assert!(!refreshed.needs_refresh(11_000));
        assert!(refreshed.needs_refresh(10_000 + 3_600_000));
    }

    #[test]
    fn credential_json_round_trips_with_camel_case_fields() {
        let credential = OAuthCredential {
            access_token: "access".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at_ms: 123,
            project_id: None,
        };
        let json = serde_json::to_string(&credential).expect("serialize");
        assert!(json.contains("\"accessToken\":\"access\""));
        assert!(!json.contains("projectId"));
        let parsed: OAuthCredential = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.expires_at_ms, 123);
    }
}
