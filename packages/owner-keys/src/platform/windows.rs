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

/*
CDXC:Security 2026-11-09 WHY:
GitHub #520: on a Windows PC without Windows Hello, nothing could be approved there at all (the
pairing of the computer controlling it, letting it act as the owner, letting agents reach across),
so remote control never worked unless the user set up a Hello PIN they did not want. The guide
promises the system password too, as on macOS (the Mac password) and Linux (polkit). When Windows
Hello is not available, Ghostex now shows Windows' own sign-in prompt for the signed-in account and
accepts only that account's correct password (checked with `LogonUserW`, and the account must be
the one Ghostex runs as). The password is never kept: it is wiped as soon as it was checked.
*/
/// Shows the Windows Hello prompt (face, fingerprint or PIN) with `reason`, owned by the window
/// in front (Ghostex, which just asked), and waits for the answer. Where Windows Hello isn't set
/// up, asks for the Windows account password instead.
pub(crate) fn confirm_presence(reason: &str) -> Result<Presence, KeyError> {
    match confirm_with_windows_hello(reason) {
        Err(KeyError::PresenceUnavailable(_)) => confirm_with_account_password(reason),
        answer => answer,
    }
}

fn confirm_with_windows_hello(reason: &str) -> Result<Presence, KeyError> {
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

/// How many wrong passwords the prompt takes before it gives up.
const PASSWORD_ATTEMPTS: u32 = 3;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Windows' sign-in prompt for the signed-in account; `Ok` once its correct password was typed.
fn confirm_with_account_password(reason: &str) -> Result<Presence, KeyError> {
    use windows_sys::Win32::{
        Foundation::{ERROR_CANCELLED, ERROR_LOGON_FAILURE, ERROR_SUCCESS},
        Security::Credentials::{
            CredUIPromptForWindowsCredentialsW, CREDUIWIN_ENUMERATE_CURRENT_USER, CREDUI_INFOW,
        },
        System::Com::CoTaskMemFree,
    };
    let caption = wide("Ghostex");
    let message = wide(&format!("Enter your Windows password to {reason}."));
    let window = unsafe { GetForegroundWindow() };
    let info = CREDUI_INFOW {
        cbSize: std::mem::size_of::<CREDUI_INFOW>() as u32,
        hwndParent: window.0,
        pszMessageText: message.as_ptr(),
        pszCaptionText: caption.as_ptr(),
        hbmBanner: std::ptr::null_mut(),
    };
    let mut last_error = 0u32;
    for _ in 0..PASSWORD_ATTEMPTS {
        let mut package = 0u32;
        let mut buffer: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut buffer_size = 0u32;
        let mut save = 0;
        let answer = unsafe {
            CredUIPromptForWindowsCredentialsW(
                &info,
                last_error,
                &mut package,
                std::ptr::null(),
                0,
                &mut buffer,
                &mut buffer_size,
                &mut save,
                CREDUIWIN_ENUMERATE_CURRENT_USER,
            )
        };
        if answer == ERROR_CANCELLED {
            return Err(KeyError::Cancelled);
        }
        if answer != ERROR_SUCCESS || buffer.is_null() {
            return Err(KeyError::PresenceUnavailable(format!(
                "the Windows sign-in prompt failed ({answer})"
            )));
        }
        let verified = unsafe {
            let bytes = std::slice::from_raw_parts_mut(buffer.cast::<u8>(), buffer_size as usize);
            let verified = signed_in_account_password(buffer, buffer_size);
            bytes.fill(0);
            CoTaskMemFree(buffer);
            verified
        };
        if verified {
            return Ok(Presence::WindowsPassword);
        }
        last_error = ERROR_LOGON_FAILURE;
    }
    Err(KeyError::Cancelled)
}

/// Whether the sign-in prompt's `buffer` holds the correct password of the account Ghostex runs
/// as. Everything read out of it is wiped before this returns.
unsafe fn signed_in_account_password(buffer: *const core::ffi::c_void, size: u32) -> bool {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE},
        Security::{
            Credentials::{
                CredUIParseUserNameW, CredUnPackAuthenticationBufferW,
                CRED_PACK_PROTECTED_CREDENTIALS,
            },
            LogonUserW, LOGON32_LOGON_INTERACTIVE, LOGON32_PROVIDER_DEFAULT,
        },
    };
    const FIELD: usize = 514;
    let mut name = [0u16; FIELD];
    let mut domain = [0u16; FIELD];
    let mut password = [0u16; FIELD];
    let (mut name_len, mut domain_len, mut password_len) =
        (FIELD as u32, FIELD as u32, FIELD as u32);
    let unpacked = CredUnPackAuthenticationBufferW(
        CRED_PACK_PROTECTED_CREDENTIALS,
        buffer,
        size,
        name.as_mut_ptr(),
        &mut name_len,
        domain.as_mut_ptr(),
        &mut domain_len,
        password.as_mut_ptr(),
        &mut password_len,
    ) != 0;
    let mut verified = false;
    if unpacked {
        // `MicrosoftAccount\you@outlook.com` or `PC\you`: LogonUserW takes the two halves.
        let mut user = [0u16; FIELD];
        let mut user_domain = [0u16; FIELD];
        let parsed = CredUIParseUserNameW(
            name.as_ptr(),
            user.as_mut_ptr(),
            FIELD as u32,
            user_domain.as_mut_ptr(),
            FIELD as u32,
        ) == 0;
        let (user_ptr, domain_ptr) = if parsed {
            (
                user.as_ptr(),
                if user_domain[0] == 0 {
                    std::ptr::null()
                } else {
                    user_domain.as_ptr()
                },
            )
        } else {
            (name.as_ptr(), std::ptr::null())
        };
        let mut token: HANDLE = std::ptr::null_mut();
        if LogonUserW(
            user_ptr,
            domain_ptr,
            password.as_ptr(),
            LOGON32_LOGON_INTERACTIVE,
            LOGON32_PROVIDER_DEFAULT,
            &mut token,
        ) != 0
        {
            verified = same_account_as_this_process(token);
            CloseHandle(token);
        }
        user.fill(0);
        user_domain.fill(0);
    }
    name.fill(0);
    domain.fill(0);
    password.fill(0);
    verified
}

/// Whether logon `token` is the account this process runs as (another account's password proves
/// nothing about the owner).
unsafe fn same_account_as_this_process(token: windows_sys::Win32::Foundation::HANDLE) -> bool {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE},
        Security::{EqualSid, GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER},
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    fn user_sid(token: HANDLE) -> Option<Vec<u8>> {
        let mut needed = 0u32;
        unsafe { GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed) };
        if needed == 0 {
            return None;
        }
        let mut info = vec![0u8; needed as usize];
        (unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                info.as_mut_ptr().cast(),
                needed,
                &mut needed,
            )
        } != 0)
            .then_some(info)
    }
    let mut own: HANDLE = std::ptr::null_mut();
    if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut own) == 0 {
        return false;
    }
    let same = match (user_sid(own), user_sid(token)) {
        (Some(mine), Some(theirs)) => {
            let mine = &*mine.as_ptr().cast::<TOKEN_USER>();
            let theirs = &*theirs.as_ptr().cast::<TOKEN_USER>();
            EqualSid(mine.User.Sid, theirs.User.Sid) != 0
        }
        _ => false,
    };
    CloseHandle(own);
    same
}
