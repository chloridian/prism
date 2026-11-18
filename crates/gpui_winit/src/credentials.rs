//! Credentials in the Windows Credential Manager, under the same target names as the native Windows platform.

use anyhow::{Result, anyhow};
use itertools::Itertools as _;
use windows::{
    Win32::{
        Foundation::ERROR_NOT_FOUND,
        Security::Credentials::{
            CRED_FLAGS, CRED_MAX_CREDENTIAL_BLOB_SIZE, CRED_PERSIST_LOCAL_MACHINE,
            CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree, CredReadW, CredWriteW,
        },
        System::SystemInformation::GetSystemTimeAsFileTime,
    },
    core::{PCWSTR, PWSTR},
};

pub(crate) fn write(url: &str, username: &str, password: &[u8]) -> Result<()> {
    // CredWriteW rejects larger blobs with the opaque RPC error 0x800706F7.
    if password.len() > CRED_MAX_CREDENTIAL_BLOB_SIZE as usize {
        return Err(anyhow!(
            "credential for {url} is {} bytes, which exceeds the Windows Credential Manager limit of {CRED_MAX_CREDENTIAL_BLOB_SIZE} bytes",
            password.len()
        ));
    }
    let mut username = wide(username);
    let mut target_name = wide(&target_name(url));
    let credentials = CREDENTIALW {
        LastWritten: unsafe { GetSystemTimeAsFileTime() },
        Flags: CRED_FLAGS(0),
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR::from_raw(target_name.as_mut_ptr()),
        CredentialBlobSize: password.len() as u32,
        CredentialBlob: password.as_ptr() as *mut _,
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: PWSTR::from_raw(username.as_mut_ptr()),
        ..CREDENTIALW::default()
    };
    unsafe { CredWriteW(&credentials, 0) }.map_err(|error| {
        anyhow!("failed to write credentials to the Windows Credential Manager: {error}")
    })
}

pub(crate) fn read(url: &str) -> Result<Option<(String, Vec<u8>)>> {
    let target_name = wide(&target_name(url));
    let mut credentials: *mut CREDENTIALW = std::ptr::null_mut();
    let result = unsafe {
        CredReadW(
            PCWSTR::from_raw(target_name.as_ptr()),
            CRED_TYPE_GENERIC,
            None,
            &mut credentials,
        )
    };
    if let Err(error) = result {
        if error.code() == ERROR_NOT_FOUND.to_hresult() {
            return Ok(None);
        }
        return Err(error.into());
    }
    if credentials.is_null() {
        return Ok(None);
    }

    let found = unsafe {
        let username = (*credentials).UserName.to_string();
        let blob = std::slice::from_raw_parts(
            (*credentials).CredentialBlob,
            (*credentials).CredentialBlobSize as usize,
        )
        .to_vec();
        CredFree(credentials as *const _);
        username.map(|username| (username, blob))
    };
    Ok(Some(found?))
}

pub(crate) fn delete(url: &str) -> Result<()> {
    let target_name = wide(&target_name(url));
    unsafe {
        CredDeleteW(
            PCWSTR::from_raw(target_name.as_ptr()),
            CRED_TYPE_GENERIC,
            None,
        )?
    };
    Ok(())
}

fn target_name(url: &str) -> String {
    format!("zed:url={url}")
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_names_match_the_native_windows_platform() {
        assert_eq!(
            target_name("https://example.com"),
            "zed:url=https://example.com"
        );
    }

    #[test]
    fn wide_strings_are_nul_terminated() {
        assert_eq!(wide("ab"), [b'a' as u16, b'b' as u16, 0]);
    }

    #[test]
    fn oversized_credentials_are_rejected_before_the_system_call() {
        let blob = vec![0u8; CRED_MAX_CREDENTIAL_BLOB_SIZE as usize + 1];
        let error = write("https://example.com", "user", &blob).unwrap_err();
        assert!(error.to_string().contains("exceeds"));
    }
}
