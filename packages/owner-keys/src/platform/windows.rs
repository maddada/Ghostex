//! Windows: Credential Manager and Windows Hello (`UserConsentVerifier`).

use windows::{
    core::{factory, HSTRING},
    Security::Credentials::UI::{UserConsentVerificationResult, UserConsentVerifier},
    Win32::{
        System::WinRT::IUserConsentVerifierInterop, UI::WindowsAndMessaging::GetForegroundWindow,
    },
};
use windows_future::IAsyncOperation;
use windows_sys::Win32::{
    Foundation::{GetLastError, ERROR_NOT_FOUND},
    Security::Credentials::{
        CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    },
};

use super::SERVICE;
use crate::{KeyError, Presence};

fn target(account: &str) -> Vec<u16> {
    format!("{SERVICE}/{account}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
}

pub(crate) fn read_secret(account: &str) -> Result<Option<Vec<u8>>, KeyError> {
    let target = target(account);
    let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
    if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } == 0 {
        let error = unsafe { GetLastError() };
        if error == ERROR_NOT_FOUND {
            return Ok(None);
        }
        return Err(KeyError::KeyStore(format!("CredReadW failed ({error})")));
    }
    let secret = unsafe {
        let credential = &*credential;
        std::slice::from_raw_parts(
            credential.CredentialBlob,
            credential.CredentialBlobSize as usize,
        )
        .to_vec()
    };
    unsafe { CredFree(credential.cast()) };
    Ok(Some(secret))
}

pub(crate) fn write_secret(account: &str, secret: &[u8]) -> Result<(), KeyError> {
    let target = target(account);
    let mut blob = secret.to_vec();
    let mut credential: CREDENTIALW = unsafe { std::mem::zeroed() };
    credential.Type = CRED_TYPE_GENERIC;
    credential.TargetName = target.as_ptr() as *mut u16;
    credential.CredentialBlobSize = blob.len() as u32;
    credential.CredentialBlob = blob.as_mut_ptr();
    credential.Persist = CRED_PERSIST_LOCAL_MACHINE;
    if unsafe { CredWriteW(&credential, 0) } == 0 {
        let error = unsafe { GetLastError() };
        return Err(KeyError::KeyStore(format!("CredWriteW failed ({error})")));
    }
    Ok(())
}

/// Shows the Windows Hello prompt (face, fingerprint or PIN) with `reason`, owned by the window
/// in front (Ghostex, which just asked), and waits for the answer.
pub(crate) fn confirm_presence(reason: &str) -> Result<Presence, KeyError> {
    let unavailable = |error: windows::core::Error| KeyError::PresenceUnavailable(error.message());
    let interop =
        factory::<UserConsentVerifier, IUserConsentVerifierInterop>().map_err(unavailable)?;
    let window = unsafe { GetForegroundWindow() };
    let operation: IAsyncOperation<UserConsentVerificationResult> =
        unsafe { interop.RequestVerificationForWindowAsync(window, &HSTRING::from(reason)) }
            .map_err(unavailable)?;
    match operation.join().map_err(unavailable)? {
        UserConsentVerificationResult::Verified => Ok(Presence::WindowsHello),
        UserConsentVerificationResult::Canceled
        | UserConsentVerificationResult::RetriesExhausted => Err(KeyError::Cancelled),
        other => Err(KeyError::PresenceUnavailable(format!(
            "Windows Hello isn't set up ({})",
            other.0
        ))),
    }
}
