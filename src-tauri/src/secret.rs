use std::path::Path;
#[cfg(target_os = "windows")]
use std::path::PathBuf;

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::LocalFree;
#[cfg(target_os = "windows")]
use windows_sys::Win32::Security::Cryptography::{
    CRYPTPROTECT_PROMPTSTRUCT, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

#[cfg(target_os = "windows")]
type ProtectDataFn = unsafe extern "system" fn(
    *const CRYPT_INTEGER_BLOB,
    *const u16,
    *const CRYPT_INTEGER_BLOB,
    *const std::ffi::c_void,
    *const CRYPTPROTECT_PROMPTSTRUCT,
    u32,
    *mut CRYPT_INTEGER_BLOB,
) -> i32;

#[cfg(target_os = "windows")]
type UnprotectDataFn = unsafe extern "system" fn(
    *const CRYPT_INTEGER_BLOB,
    *mut *mut u16,
    *const CRYPT_INTEGER_BLOB,
    *const std::ffi::c_void,
    *const CRYPTPROTECT_PROMPTSTRUCT,
    u32,
    *mut CRYPT_INTEGER_BLOB,
) -> i32;

const MAX_SECRET_BYTES: usize = 4096;
#[cfg(target_os = "windows")]
const MAX_CIPHERTEXT_BYTES: usize = 64 * 1024;

/// Keychain service name on non-Windows platforms. Matches the app bundle identifier.
#[cfg(not(target_os = "windows"))]
const KEYCHAIN_SERVICE: &str = "cn.ttpublic.llmusage";

#[derive(Debug)]
pub enum SecretError {
    Invalid,
    Protect,
    Io,
    Missing,
}

pub struct SecretVault {
    #[cfg(target_os = "windows")]
    path: PathBuf,
    #[cfg(not(target_os = "windows"))]
    provider_id: String,
}

impl SecretVault {
    pub fn new(app_data_dir: &Path, provider_id: &str) -> Result<Self, SecretError> {
        if !is_provider_id(provider_id) {
            return Err(SecretError::Invalid);
        }
        Ok(Self {
            #[cfg(target_os = "windows")]
            path: app_data_dir
                .join("credentials")
                .join(format!("{provider_id}.dpapi")),
            #[cfg(not(target_os = "windows"))]
            provider_id: provider_id.to_string(),
        })
    }

    #[cfg(target_os = "windows")]
    pub fn exists(&self) -> bool {
        self.path.is_file()
    }

    #[cfg(not(target_os = "windows"))]
    pub fn exists(&self) -> bool {
        match self.keychain_entry() {
            Ok(entry) => entry.get_password().is_ok(),
            Err(_) => false,
        }
    }

    #[cfg(target_os = "windows")]
    pub fn save(&self, secret: &str) -> Result<(), SecretError> {
        let ciphertext = protect(secret.as_bytes())?;
        let parent = self.path.parent().ok_or(SecretError::Io)?;
        std::fs::create_dir_all(parent).map_err(|_| SecretError::Io)?;
        std::fs::write(&self.path, ciphertext).map_err(|_| SecretError::Io)
    }

    #[cfg(not(target_os = "windows"))]
    pub fn save(&self, secret: &str) -> Result<(), SecretError> {
        if secret.is_empty() || secret.len() > MAX_SECRET_BYTES {
            return Err(SecretError::Invalid);
        }
        self.keychain_entry()?
            .set_password(secret)
            .map_err(map_keyring_error)
    }

    #[cfg(target_os = "windows")]
    pub fn load(&self) -> Result<String, SecretError> {
        let ciphertext = std::fs::read(&self.path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                SecretError::Missing
            } else {
                SecretError::Io
            }
        })?;
        if ciphertext.is_empty() || ciphertext.len() > MAX_CIPHERTEXT_BYTES {
            return Err(SecretError::Invalid);
        }
        String::from_utf8(unprotect(&ciphertext)?).map_err(|_| SecretError::Invalid)
    }

    #[cfg(not(target_os = "windows"))]
    pub fn load(&self) -> Result<String, SecretError> {
        self.keychain_entry()?
            .get_password()
            .map_err(map_keyring_error)
    }

    #[cfg(not(target_os = "windows"))]
    fn keychain_entry(&self) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(KEYCHAIN_SERVICE, &self.provider_id).map_err(map_keyring_error)
    }
}

fn is_provider_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

#[cfg(not(target_os = "windows"))]
fn map_keyring_error(error: keyring::Error) -> SecretError {
    match error {
        keyring::Error::NoEntry => SecretError::Missing,
        keyring::Error::BadEncoding(_) => SecretError::Invalid,
        _ => SecretError::Protect,
    }
}

#[cfg(target_os = "windows")]
fn protect(plaintext: &[u8]) -> Result<Vec<u8>, SecretError> {
    if plaintext.is_empty() || plaintext.len() > MAX_SECRET_BYTES {
        return Err(SecretError::Invalid);
    }
    let input = CRYPT_INTEGER_BLOB {
        cbData: plaintext.len() as u32,
        pbData: plaintext.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: The trusted system DLL is loaded by name and the symbol signature matches Win32 DPAPI.
    let succeeded = unsafe {
        let library = libloading::Library::new("crypt32.dll").map_err(|_| SecretError::Protect)?;
        let protect: libloading::Symbol<ProtectDataFn> = library
            .get(b"CryptProtectData\0")
            .map_err(|_| SecretError::Protect)?;
        protect(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    copy_and_free_output(succeeded, output)
}

#[cfg(target_os = "windows")]
fn unprotect(ciphertext: &[u8]) -> Result<Vec<u8>, SecretError> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: ciphertext.len() as u32,
        pbData: ciphertext.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: The trusted system DLL is loaded by name and the symbol signature matches Win32 DPAPI.
    let succeeded = unsafe {
        let library = libloading::Library::new("crypt32.dll").map_err(|_| SecretError::Protect)?;
        let unprotect: libloading::Symbol<UnprotectDataFn> = library
            .get(b"CryptUnprotectData\0")
            .map_err(|_| SecretError::Protect)?;
        unprotect(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    copy_and_free_output(succeeded, output)
}

#[cfg(target_os = "windows")]
fn copy_and_free_output(succeeded: i32, output: CRYPT_INTEGER_BLOB) -> Result<Vec<u8>, SecretError> {
    if succeeded == 0 || output.pbData.is_null() || output.cbData == 0 {
        return Err(SecretError::Protect);
    }
    // SAFETY: DPAPI returned a valid buffer of cbData bytes on success.
    let bytes = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    // SAFETY: The output pointer is allocated by DPAPI using LocalAlloc.
    unsafe { LocalFree(output.pbData.cast()) };
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_provider_ids_that_could_escape_credentials_dir() {
        assert!(matches!(
            SecretVault::new(Path::new("C:/app"), "../glm"),
            Err(SecretError::Invalid)
        ));
        assert!(matches!(
            SecretVault::new(Path::new("C:/app"), "GLM"),
            Err(SecretError::Invalid)
        ));
        assert!(matches!(
            SecretVault::new(Path::new("C:/app"), ""),
            Err(SecretError::Invalid)
        ));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn dpapi_round_trip_does_not_store_plaintext() {
        let plaintext = b"test-only-secret";
        let encrypted = protect(plaintext).expect("DPAPI encryption");

        assert_ne!(encrypted, plaintext);
        assert_eq!(unprotect(&encrypted).expect("DPAPI decryption"), plaintext);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn rejects_empty_secrets() {
        assert!(matches!(protect(b""), Err(SecretError::Invalid)));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn stores_each_provider_in_an_isolated_dpapi_file() {
        let app_data = Path::new("C:/Users/example/AppData/Roaming/LLMUsage");

        let glm = SecretVault::new(app_data, "glm").expect("valid provider id");
        let kimi = SecretVault::new(app_data, "kimi").expect("valid provider id");

        assert!(glm.path.ends_with(Path::new("credentials/glm.dpapi")));
        assert!(kimi.path.ends_with(Path::new("credentials/kimi.dpapi")));
        assert_ne!(glm.path, kimi.path);
    }
}
