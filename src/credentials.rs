//! Only this application's AI Hub token is read, and only on explicit AI actions.
use anyhow::{Result, ensure};
use zeroize::Zeroizing;

pub fn validate(token: &str) -> Result<()> {
    ensure!(
        token.len() <= 2048 && token.bytes().all(|b| b.is_ascii_graphic()),
        "The token must contain at most 2048 printable ASCII characters"
    );
    Ok(())
}

#[cfg(windows)]
mod windows {
    use super::*;
    use windows_sys::Win32::Security::Credentials::*;
    fn target(base: &str) -> Vec<u16> {
        use sha2::{Digest, Sha256};
        format!("FileOrganizer/AIHub/{:x}", Sha256::digest(base.as_bytes()))
            .encode_utf16()
            .chain(Some(0))
            .collect()
    }
    pub fn read(base: &str) -> Result<Option<Zeroizing<String>>> {
        let mut value = std::ptr::null_mut();
        let ok = unsafe { CredReadW(target(base).as_ptr(), CRED_TYPE_GENERIC, 0, &mut value) };
        if ok == 0 {
            let e = std::io::Error::last_os_error();
            if e.raw_os_error() == Some(1168) {
                return Ok(None);
            }
            return Err(e.into());
        }
        // Copy only our own generic credential; never enumerate the credential store.
        ensure!(!value.is_null(), "The saved AI Hub credential is invalid");
        let result = (|| -> Result<Zeroizing<String>> {
            let size = unsafe { (*value).CredentialBlobSize as usize };
            let blob = unsafe { (*value).CredentialBlob };
            ensure!(
                size > 0 && size <= 2048 && !blob.is_null(),
                "The saved AI Hub credential is invalid"
            );
            let bytes = unsafe { std::slice::from_raw_parts_mut(blob, size) };
            let copied = Zeroizing::new(bytes.to_vec());
            use zeroize::Zeroize;
            bytes.zeroize();
            let token = Zeroizing::new(std::str::from_utf8(&copied)?.to_owned());
            validate(&token)?;
            Ok(token)
        })();
        unsafe {
            CredFree(value.cast());
        }
        Ok(Some(result?))
    }
    pub fn store(base: &str, token: &str) -> Result<()> {
        validate(token)?;
        ensure!(!token.is_empty(), "Enter an AI Hub access token");
        let mut name = target(base);
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: name.as_mut_ptr(),
            CredentialBlobSize: token.len() as u32,
            CredentialBlob: token.as_ptr() as *mut u8,
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..Default::default()
        };
        ensure!(
            unsafe { CredWriteW(&credential, 0) } != 0,
            "Could not save the AI Hub token in Windows Credential Manager"
        );
        Ok(())
    }
    pub fn remove(base: &str) -> Result<()> {
        if unsafe { CredDeleteW(target(base).as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 {
            ensure!(
                std::io::Error::last_os_error().raw_os_error() == Some(1168),
                "Could not remove the saved AI Hub token"
            );
        }
        Ok(())
    }
}
#[cfg(windows)]
pub use windows::{read, remove, store};
#[cfg(not(windows))]
pub fn read(_: &str) -> Result<Option<Zeroizing<String>>> {
    Ok(None)
}
#[cfg(not(windows))]
pub fn store(_: &str, _: &str) -> Result<()> {
    anyhow::bail!("Secure token storage requires Windows")
}
#[cfg(not(windows))]
pub fn remove(_: &str) -> Result<()> {
    Ok(())
}
