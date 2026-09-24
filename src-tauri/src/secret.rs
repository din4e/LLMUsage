use std::path::{Path, PathBuf};

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
    app_data: PathBuf,
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
            app_data: app_data_dir.to_path_buf(),
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
            .map_err(map_keyring_error)?;
        // The keyring cannot be enumerated by service, so mirror this instance
        // in the registry file for list/export/import. Best effort: a registry
        // IO failure never fails an already-successful credential save.
        registry_add(&self.app_data, &self.provider_id);
        Ok(())
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

    #[cfg(target_os = "windows")]
    pub fn delete(&self) -> Result<(), SecretError> {
        // Idempotent: deleting a credential that was never saved (or already
        // removed) succeeds so the UI can treat "forget provider" uniformly.
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(SecretError::Io),
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn load(&self) -> Result<String, SecretError> {
        self.keychain_entry()?
            .get_password()
            .map_err(map_keyring_error)
    }

    #[cfg(not(target_os = "windows"))]
    pub fn delete(&self) -> Result<(), SecretError> {
        // keyring 3.x exposes `delete_credential`, which returns `NoEntry` when
        // the item never existed. Treat that as success so forgetting a provider
        // is idempotent regardless of prior keychain state.
        match self.keychain_entry()?.delete_credential() {
            Ok(()) => {
                registry_remove(&self.app_data, &self.provider_id);
                Ok(())
            }
            Err(error) => match map_keyring_error(error) {
                SecretError::Missing => Ok(()),
                other => Err(other),
            },
        }
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

/// Non-Windows platforms keep credentials in the system keyring, which cannot
/// enumerate entries by service. `instances.json` in the app data directory
/// mirrors the set of configured instance ids so listing, export, and import
/// dedup keep working; `SecretVault::save`/`delete` keep it in sync. Windows
/// never writes it — the DPAPI directory is the enumeration source there.
fn instance_registry_path(app_data_dir: &Path) -> std::path::PathBuf {
    app_data_dir.join("instances.json")
}

fn read_instance_registry(app_data_dir: &Path) -> Vec<String> {
    let Ok(bytes) = std::fs::read(instance_registry_path(app_data_dir)) else {
        return Vec::new();
    };
    let Ok(values) = serde_json::from_slice::<Vec<String>>(&bytes) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = values.into_iter().filter(|id| is_provider_id(id)).collect();
    ids.sort();
    ids.dedup();
    ids
}

// Only the non-Windows save/delete paths call the writers; Windows enumerates
// DPAPI files instead. Tests exercise them on every platform, hence the
// Windows-targeted dead_code allowance on the lib build.
#[cfg_attr(target_os = "windows", allow(dead_code))]
fn write_instance_registry(app_data_dir: &Path, ids: &[String]) {
    let path = instance_registry_path(app_data_dir);
    let Some(parent) = path.parent() else { return };
    let _ = std::fs::create_dir_all(parent);
    if let Ok(bytes) = serde_json::to_vec(ids) {
        let _ = std::fs::write(path, bytes);
    }
}

/// Records a configured instance id. Best effort, idempotent.
#[cfg_attr(target_os = "windows", allow(dead_code))]
fn registry_add(app_data_dir: &Path, instance_id: &str) {
    if !is_provider_id(instance_id) {
        return;
    }
    let mut ids = read_instance_registry(app_data_dir);
    if !ids.iter().any(|id| id == instance_id) {
        ids.push(instance_id.to_string());
        write_instance_registry(app_data_dir, &ids);
    }
}

/// Drops an instance id after its credential was deleted. Best effort.
#[cfg_attr(target_os = "windows", allow(dead_code))]
fn registry_remove(app_data_dir: &Path, instance_id: &str) {
    let mut ids = read_instance_registry(app_data_dir);
    let before = ids.len();
    ids.retain(|id| id != instance_id);
    if ids.len() != before {
        write_instance_registry(app_data_dir, &ids);
    }
}

/// Instance ids recorded by non-Windows credential saves, sorted and deduped.
/// Empty when the registry file is absent (the Windows case).
pub fn registry_ids(app_data_dir: &Path) -> Vec<String> {
    read_instance_registry(app_data_dir)
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
    #[cfg(target_os = "windows")]
    fn stores_each_instance_of_the_same_provider_in_its_own_dpapi_file() {
        let app_data = Path::new("C:/Users/example/AppData/Roaming/LLMUsage");

        let first = SecretVault::new(app_data, "kimi_cn").expect("valid instance id");
        let second = SecretVault::new(app_data, "kimi_cn_2").expect("valid instance id");

        assert!(first.path.ends_with(Path::new("credentials/kimi_cn.dpapi")));
        assert!(second.path.ends_with(Path::new("credentials/kimi_cn_2.dpapi")));
        assert_ne!(first.path, second.path);
    }

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

    #[test]
    fn instance_registry_round_trips_add_and_remove() {
        let dir = std::env::temp_dir().join(format!(
            "llm-usage-registry-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);

        registry_add(&dir, "kimi_cn");
        registry_add(&dir, "kimi_cn_2");
        registry_add(&dir, "kimi_cn"); // idempotent
        registry_add(&dir, "../evil"); // unsafe ids are ignored
        assert_eq!(registry_ids(&dir), vec!["kimi_cn", "kimi_cn_2"]);

        registry_remove(&dir, "kimi_cn");
        registry_remove(&dir, "kimi_cn"); // idempotent
        assert_eq!(registry_ids(&dir), vec!["kimi_cn_2"]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn instance_registry_tolerates_a_corrupt_file() {
        let dir = std::env::temp_dir().join(format!(
            "llm-usage-registry-corrupt-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(dir.join("instances.json"), b"{not-json").expect("seed corrupt");

        assert!(registry_ids(&dir).is_empty());
        registry_add(&dir, "glm");
        assert_eq!(registry_ids(&dir), vec!["glm"]);

        let _ = std::fs::remove_dir_all(&dir);
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

    #[cfg(target_os = "windows")]
    #[test]
    fn deletes_a_stored_dpapi_credential_and_is_idempotent() {
        let dir = std::env::temp_dir()
            .join(format!("llm-usage-secret-delete-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let vault = SecretVault::new(&dir, "kimi").expect("valid provider id");

        vault.save("test-only-secret").expect("save secret");
        assert!(vault.exists());

        vault.delete().expect("delete removes the stored credential");
        assert!(!vault.exists());

        // Forgetting a provider must be safe to repeat.
        vault.delete().expect("delete is idempotent when already gone");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
