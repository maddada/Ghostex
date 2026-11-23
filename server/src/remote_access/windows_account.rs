//! The Windows account gxserver runs as: its `DOMAIN\user` name and whether it is a Microsoft
//! Entra ID (Azure AD) work or school account, which decides the SSH user name phones get and
//! whether a phone can pair here at all.
//!
//! CDXC:RemotePairing 2026-11-23 DECISION:
//! User (GitHub #716, an Android phone pairing with an Entra ID joined Windows 11 PC): "investigate
//! and detect": hand the phone the user name form that works, and detect Entra ID accounts with a
//! clear message when a phone can't pair. What Windows OpenSSH Server does with an Entra account:
//! it answers "Invalid user" to the bare name (what `USERNAME` holds) and to the UPN; only
//! `AzureAD\user` (the name `whoami` prints) resolves, so that is the user name Ghostex hands out
//! for these accounts. Key sign-in still can't work: Microsoft's OpenSSH docs say "Microsoft Entra
//! ID accounts don't support key-based authentication", because Windows won't make the S4U token
//! sshd needs for one (PowerShell/Win32-OpenSSH#1787); the inbox 9.5 sshd even drops `AzureAD\…`
//! connections before any method is tried (the #716 reporter, and #1787), and only newer builds
//! accept a password. A phone paired by QR signs in with its key alone, so `begin_pairing` refuses
//! it up front with [`ENTRA_ID_PHONE_REFUSAL`] instead of asking for the words and the
//! administrator prompt and then failing as "SSH needs a key or password". Microsoft names the SID
//! prefix S-1-12-1 as the sign of an Entra account (#1787), so that is what is checked.
//! SEE-ALSO: server/src/remote_access/identity.rs (`read_login_username`),
//! server/src/remote_access/pair_device.rs (`begin_pairing`).

/// What a phone hears when it scans a code on a computer signed in with an Entra ID account.
pub const ENTRA_ID_PHONE_REFUSAL: &str = "This computer is signed in to Windows with a work or school account (Microsoft Entra ID), and Windows' SSH server can't let a phone sign in to those accounts with a key, so this phone can't connect to it. To use a phone with Ghostex here, sign in to this computer with a local account or a personal Microsoft account.";

/// The account gxserver runs as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowsAccount {
    /// `DOMAIN\user` (`AzureAD\DaveWilson` for an Entra ID account).
    pub qualified_name: String,
    /// The account's SID, `S-1-…`.
    pub sid: String,
    pub entra_id: bool,
}

/// Whether `sid` (`S-1-…`) is a Microsoft Entra ID account's: identifier authority 12, first
/// sub-authority 1.
pub fn is_entra_id_sid(sid: &str) -> bool {
    sid.trim().to_ascii_uppercase().starts_with("S-1-12-1-")
}

/// The account gxserver runs as, read once: it can't change while gxserver runs. `None` off
/// Windows, or when Windows would not say.
pub fn this_windows_account() -> Option<&'static WindowsAccount> {
    static ACCOUNT: std::sync::OnceLock<Option<WindowsAccount>> = std::sync::OnceLock::new();
    ACCOUNT.get_or_init(read_windows_account).as_ref()
}

/// Whether gxserver runs as a Microsoft Entra ID account.
pub fn this_account_is_entra_id() -> bool {
    this_windows_account().is_some_and(|account| account.entra_id)
}

#[cfg(not(windows))]
fn read_windows_account() -> Option<WindowsAccount> {
    None
}

#[cfg(windows)]
fn read_windows_account() -> Option<WindowsAccount> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::{
        Security::{
            GetSidIdentifierAuthority, GetSidSubAuthority, GetSidSubAuthorityCount,
            GetTokenInformation, LookupAccountSidW, TokenUser, SID_NAME_USE, TOKEN_QUERY,
            TOKEN_USER,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };

    let mut raw_token = std::ptr::null_mut();
    // SAFETY: the current process's pseudo handle, and an out pointer that lives for the call.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw_token) } == 0 {
        return None;
    }
    // SAFETY: OpenProcessToken succeeded, so `raw_token` is a handle this code now owns.
    let token = unsafe { OwnedHandle::from_raw_handle(raw_token) };
    let mut length = 0u32;
    // SAFETY: a null buffer with length 0 only asks for the size.
    unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            std::ptr::null_mut(),
            0,
            &mut length,
        )
    };
    if length == 0 {
        return None;
    }
    // u64s keep the TOKEN_USER at the start of the buffer aligned.
    let mut buffer = vec![0u64; (length as usize).div_ceil(std::mem::size_of::<u64>())];
    // SAFETY: the buffer holds `length` bytes and stays alive for the call.
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &mut length,
        )
    } == 0
    {
        return None;
    }
    // SAFETY: GetTokenInformation(TokenUser) filled the buffer with a TOKEN_USER whose SID points
    // into the same buffer, which outlives every use below.
    let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    // SAFETY: `sid` is the valid SID read above; these only read from it.
    let sid_text = unsafe {
        let authority = (*GetSidIdentifierAuthority(sid)).Value;
        let authority = authority
            .iter()
            .fold(0u64, |value, byte| (value << 8) | u64::from(*byte));
        let count = *GetSidSubAuthorityCount(sid);
        let mut text = format!("S-1-{authority}");
        for index in 0..u32::from(count) {
            text.push_str(&format!("-{}", *GetSidSubAuthority(sid, index)));
        }
        text
    };

    let mut name = vec![0u16; 256];
    let mut domain = vec![0u16; 256];
    let mut name_length = name.len() as u32;
    let mut domain_length = domain.len() as u32;
    let mut sid_use: SID_NAME_USE = 0;
    // SAFETY: both buffers hold the lengths passed and stay alive for the call.
    let looked_up = unsafe {
        LookupAccountSidW(
            std::ptr::null(),
            sid,
            name.as_mut_ptr(),
            &mut name_length,
            domain.as_mut_ptr(),
            &mut domain_length,
            &mut sid_use,
        )
    } != 0;
    let qualified_name = if looked_up {
        let name = String::from_utf16_lossy(&name[..name_length as usize]);
        let domain = String::from_utf16_lossy(&domain[..domain_length as usize]);
        format!("{domain}\\{name}")
    } else {
        // An Entra account Windows can't look up right now (no cached sign-in): Windows names its
        // domain `AzureAD` in the environment of the user's own processes too.
        format!(
            "{}\\{}",
            std::env::var("USERDOMAIN").ok()?.trim(),
            std::env::var("USERNAME").ok()?.trim()
        )
    };
    Some(WindowsAccount {
        qualified_name,
        entra_id: is_entra_id_sid(&sid_text),
        sid: sid_text,
    })
}
