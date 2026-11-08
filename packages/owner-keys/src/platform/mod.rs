//! The key store and the presence prompt of each platform.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub(crate) use linux::*;
#[cfg(target_os = "macos")]
pub(crate) use macos::*;
#[cfg(target_os = "windows")]
pub(crate) use windows::*;

/// The key store's service / target prefix for both keys.
#[allow(dead_code)]
pub(crate) const SERVICE: &str = "dev.ghostex.keys";

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod unsupported {
    use crate::{KeyError, Presence};

    pub(crate) fn read_secret(_account: &str) -> Result<Option<Vec<u8>>, KeyError> {
        Err(KeyError::KeyStore("no key store on this platform".into()))
    }
    pub(crate) fn write_secret(_account: &str, _secret: &[u8]) -> Result<(), KeyError> {
        Err(KeyError::KeyStore("no key store on this platform".into()))
    }
    pub(crate) fn confirm_presence(_reason: &str) -> Result<Presence, KeyError> {
        Err(KeyError::PresenceUnavailable("unsupported platform".into()))
    }
}
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub(crate) use unsupported::*;
