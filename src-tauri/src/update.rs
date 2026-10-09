use std::io::Write;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::app::CommandError;

const RELEASES_LATEST_API: &str = "https://api.github.com/repos/din4e/LLMUsage/releases/latest";
const UPDATE_PROGRESS_EVENT: &str = "update-progress";
/// Matches the user-agent convention the provider clients already use.
const USER_AGENT: &str = "LLMUsage/0.1";
const REQUEST_TIMEOUT_SECONDS: u64 = 15;
/// Hard cap so a compromised/huge asset cannot fill the disk; the real NSIS
/// bundle is ~2 MiB.
const MAX_INSTALLER_BYTES: u64 = 64 * 1024 * 1024;

/// Result of a release check, mirrored into the frontend camelCase.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub download_url: Option<String>,
    pub download_size: Option<u64>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct UpdateProgress {
    downloaded: u64,
    total: u64,
}

fn update_client() -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .https_only(true)
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
        .user_agent(USER_AGENT);
    if let Some(proxy) = system_http_proxy() {
        if let Ok(proxy) = reqwest::Proxy::all(&proxy) {
            builder = builder.proxy(proxy);
        }
    }
    builder.build().unwrap_or_default()
}

/// Reads the WinINET "Internet Settings" proxy the way browsers do. Desktop
/// users rarely export http_proxy env vars, so without this the update check
/// fails on any machine that reaches GitHub through a system proxy.
#[cfg(target_os = "windows")]
fn system_http_proxy() -> Option<String> {
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
    };

    fn registry_value(name: &[u16], kind: u32) -> Option<(Vec<u8>, u32)> {
        let subkey: Vec<u16> = utf16(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings");
        let mut data = [0u8; 512];
        let mut size = data.len() as u32;
        let mut kind_out = 0u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                name.as_ptr(),
                kind,
                &mut kind_out,
                data.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status == 0 {
            Some((data[..size as usize].to_vec(), kind_out))
        } else {
            None
        }
    }

    let enabled = registry_value(&utf16("ProxyEnable"), RRF_RT_REG_DWORD)
        .and_then(|(bytes, _)| bytes.first().map(|byte| *byte != 0))?;
    if !enabled {
        return None;
    }
    let server = registry_value(&utf16("ProxyServer"), RRF_RT_REG_SZ).and_then(|(bytes, _)| {
        Some(
            String::from_utf16_lossy(
                &bytes
                    .chunks_exact(2)
                    .take_while(|pair| !(pair[0] == 0 && pair[1] == 0))
                    .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                    .collect::<Vec<u16>>(),
            ),
        )
    })?;
    // "host:port" applies to every protocol; "http=h:p;https=h:p;…" per
    // protocol — prefer the https entry, fall back to the first mapping.
    let candidate = if server.contains('=') {
        server
            .split(';')
            .find(|part| part.trim_start().starts_with("https="))
            .or_else(|| server.split(';').find(|part| part.contains('=')))?
            .split_once('=')
            .map(|(_, host)| host.trim().to_string())?
    } else {
        server.trim().to_string()
    };
    if candidate.is_empty() {
        None
    } else {
        Some(if candidate.starts_with("http") {
            candidate
        } else {
            format!("http://{candidate}")
        })
    }
}

#[cfg(not(target_os = "windows"))]
fn system_http_proxy() -> Option<String> {
    // Unix desktops export proxy env vars far more often; reqwest already
    // picks those up natively.
    None
}

fn utf16(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// "v0.1.10" / "0.1.10" → (0, 1, 10); unknown shapes compare as (0, 0, 0).
fn version_tuple(value: &str) -> (u64, u64, u64) {
    let mut parts = value
        .trim()
        .trim_start_matches(|c: char| !c.is_ascii_digit())
        .split('.');
    let mut next = || parts.next().and_then(|p| p.split('-').next()).and_then(|p| p.parse().ok()).unwrap_or(0);
    (next(), next(), next())
}

/// Picks the Windows x64 NSIS asset from a GitHub release JSON payload.
fn nsis_asset(release: &serde_json::Value) -> Option<(String, u64)> {
    let assets = release.get("assets")?.as_array()?;
    let asset = assets.iter().find(|asset| {
        let name = asset.get("name").and_then(|v| v.as_str()).unwrap_or_default();
        name.ends_with("x64-setup.exe") && !name.contains("aarch64")
    })?;
    let url = asset.get("browser_download_url").and_then(|v| v.as_str())?.to_string();
    let size = asset.get("size").and_then(|v| v.as_u64()).unwrap_or_default();
    Some((url, size))
}

/// Queries GitHub for the latest release and compares it against the running
/// build. Network/parse failures map onto the update-specific error so the
/// UI can point at proxy settings instead of implying the app is broken.
#[tauri::command(rename_all = "camelCase")]
pub async fn check_for_update(app: AppHandle) -> Result<UpdateCheckResult, CommandError> {
    let current_version = app.package_info().version.to_string();
    let result = fetch_latest_release()
        .await
        .map_err(|_| CommandError::update_check_failed())?;
    let latest_version = result
        .get("tag_name")
        .and_then(|v| v.as_str())
        // Tags carry a "v" prefix ("v0.1.10"); strip it so the frontend can
        // render one consistent "v{version}" label.
        .map(|tag| tag.trim_start_matches('v').trim().to_string())
        .ok_or_else(CommandError::update_check_failed)?;
    let (download_url, download_size) = match nsis_asset(&result) {
        Some((url, size)) => (Some(url), Some(size)),
        None => (None, None),
    };
    let update_available = version_tuple(&latest_version) > version_tuple(&current_version);
    Ok(UpdateCheckResult {
        current_version,
        latest_version,
        update_available,
        download_url,
        download_size,
    })
}

async fn fetch_latest_release() -> Result<serde_json::Value, reqwest::Error> {
    update_client()
        .get(RELEASES_LATEST_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
}

/// Downloads the NSIS bundle to %TEMP% with progress events, then hands off
/// to the installer (`/S /UPDATE /R`: silent upgrade, restart the app after)
/// and exits — the same hand-off dance the official updater plugin uses. The
/// NSIS template kills the running instance itself before replacing files.
#[tauri::command(rename_all = "camelCase")]
pub async fn download_and_install_update(
    app: AppHandle,
    url: String,
    expected_size: u64,
) -> Result<(), CommandError> {
    if url.trim().is_empty() || !url.starts_with("https://") {
        return Err(CommandError::update_download_failed());
    }
    let expected_size = if expected_size > 0 {
        expected_size
    } else {
        MAX_INSTALLER_BYTES
    };
    if expected_size > MAX_INSTALLER_BYTES {
        return Err(CommandError::update_download_failed());
    }
    let file_name = url.rsplit('/').next().unwrap_or("llm-usage-setup.exe");
    let installer_path = std::env::temp_dir().join(file_name);

    let mut response = update_client()
        .get(&url)
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|_| CommandError::update_download_failed())?;
    let mut file =
        std::fs::File::create(&installer_path).map_err(|_| CommandError::update_download_failed())?;
    let mut downloaded: u64 = 0;
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                downloaded += chunk.len() as u64;
                if downloaded > expected_size {
                    drop(file);
                    let _ = std::fs::remove_file(&installer_path);
                    return Err(CommandError::update_download_failed());
                }
                file.write_all(&chunk)
                    .map_err(|_| CommandError::update_download_failed())?;
                let _ = app.emit(
                    UPDATE_PROGRESS_EVENT,
                    UpdateProgress {
                        downloaded,
                        total: expected_size,
                    },
                );
            }
            // EOF...
            Ok(None) => break,
            // ...or a transport teardown after the final byte (proxies some-
            // times reset instead of closing cleanly). The exact-size check
            // below is the real integrity gate, so tolerate it.
            Err(_) if downloaded == expected_size => break,
            Err(_) => {
                drop(file);
                let _ = std::fs::remove_file(&installer_path);
                return Err(CommandError::update_download_failed());
            }
        }
    }
    file.flush().map_err(|_| CommandError::update_download_failed())?;
    if downloaded != expected_size {
        drop(file);
        let _ = std::fs::remove_file(&installer_path);
        return Err(CommandError::update_download_failed());
    }
    // Close the write handle before spawning: Windows refuses to execute an
    // image that is still open for writing (sharing violation → spawn fails
    // and the whole upgrade reports a download error).
    drop(file);

    launch_installer(&installer_path).map_err(|_| CommandError::update_download_failed())
}

fn launch_installer(installer_path: &std::path::Path) -> Result<(), std::io::Error> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // Detached so the installer survives our immediate exit.
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        std::process::Command::new(installer_path)
            .args(["/S", "/UPDATE", "/R"])
            .creation_flags(DETACHED_PROCESS)
            .spawn()?;
        std::process::exit(0);
    }
    #[cfg(not(target_os = "windows"))]
    {
        // macOS/Linux bundles: replace-via-installer is a Windows-only flow
        // for now; surface it as an error instead of silently exiting.
        let _ = installer_path;
        Err(std::io::Error::other("自动更新目前仅支持 Windows 安装包"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions_and_orders_them() {
        assert_eq!(version_tuple("v0.1.10"), (0, 1, 10));
        assert_eq!(version_tuple("0.2.0"), (0, 2, 0));
        assert_eq!(version_tuple("1.2.3-beta.1"), (1, 2, 3));
        assert_eq!(version_tuple("garbage"), (0, 0, 0));
        assert!(version_tuple("v0.1.10") > version_tuple("v0.1.9"));
        assert!(version_tuple("0.2.0") > version_tuple("0.1.99"));
        assert!(!(version_tuple("v0.1.10") > version_tuple("v0.1.10")));
    }

    #[test]
    fn picks_only_the_windows_x64_nsis_asset() {
        let release = serde_json::json!({
            "assets": [
                { "name": "LLM.Usage_0.1.10_aarch64.dmg", "browser_download_url": "https://x/dmg", "size": 1 },
                { "name": "LLM.Usage_aarch64.app.tar.gz", "browser_download_url": "https://x/tgz", "size": 2 },
                { "name": "LLM.Usage_0.1.10_x64-setup.exe", "browser_download_url": "https://x/exe", "size": 2076054 },
            ]
        });
        let (url, size) = nsis_asset(&release).expect("asset found");
        assert_eq!(url, "https://x/exe");
        assert_eq!(size, 2_076_054);
        assert!(nsis_asset(&serde_json::json!({ "assets": [] })).is_none());
    }

    #[test]
    fn mirrors_the_check_result_in_camel_case() {
        let payload = UpdateCheckResult {
            current_version: "0.1.9".to_string(),
            latest_version: "0.1.10".to_string(),
            update_available: true,
            download_url: Some("https://x/exe".to_string()),
            download_size: Some(42),
        };
        let json = serde_json::to_string(&payload).expect("serialize");
        assert!(json.contains("\"currentVersion\":\"0.1.9\""));
        assert!(json.contains("\"updateAvailable\":true"));
        assert!(json.contains("\"downloadUrl\""));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn proxy_or_none_without_panicking() {
        // Either a proxy string or None; the registry read must never panic.
        let _ = system_http_proxy();
    }
}
