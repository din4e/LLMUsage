use std::path::{Path, PathBuf};
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{
    CRYPTPROTECT_PROMPTSTRUCT, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

type ProtectDataFn = unsafe extern "system" fn(
    *const CRYPT_INTEGER_BLOB,
    *const u16,
    *const CRYPT_INTEGER_BLOB,
    *const std::ffi::c_void,
    *const CRYPTPROTECT_PROMPTSTRUCT,
    u32,
    *mut CRYPT_INTEGER_BLOB,
) -> i32;

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
const MAX_CIPHERTEXT_BYTES: usize = 64 * 1024;

#[derive(Debug)]
pub enum SecretError {
    Invalid,
    Protect,
    Io,
    Missing,
}

pub struct SecretVault {
    path: PathBuf,
}

impl SecretVault {
    pub fn new(app_data_dir: &Path) -> Self {
        Self {
            path: app_data_dir.join("credentials").join("glm.dpapi"),
        }
    }

    pub fn exists(&self) -> bool {
        self.path.is_file()
    }

    pub fn save(&self, secret: &str) -> Result<(), SecretError> {
        let ciphertext = protect(secret.as_bytes())?;
        let parent = self.path.parent().ok_or(SecretError::Io)?;
        std::fs::create_dir_all(parent).map_err(|_| SecretError::Io)?;
        std::fs::write(&self.path, ciphertext).map_err(|_| SecretError::Io)
    }

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
}

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

fn copy_and_free_output(
    succeeded: i32,
    output: CRYPT_INTEGER_BLOB,
) -> Result<Vec<u8>, SecretError> {
    if succeeded == 0 || output.pbData.is_null() || output.cbData == 0 {
        return Err(SecretError::Protect);
    }
    // SAFETY: DPAPI returned a valid buffer of cbData bytes on success.
    let bytes =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    // SAFETY: The output pointer is allocated by DPAPI using LocalAlloc.
    unsafe { LocalFree(output.pbData.cast()) };
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpapi_round_trip_does_not_store_plaintext() {
        let plaintext = b"test-only-secret";
        let encrypted = protect(plaintext).expect("DPAPI encryption");

        assert_ne!(encrypted, plaintext);
        assert_eq!(unprotect(&encrypted).expect("DPAPI decryption"), plaintext);
    }

    #[test]
    fn rejects_empty_secrets() {
        assert!(matches!(protect(b""), Err(SecretError::Invalid)));
    }
}
