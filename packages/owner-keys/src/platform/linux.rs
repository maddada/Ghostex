//! Linux: the Secret Service (oo7) and polkit's password prompt.

use std::process::Command;

use super::SERVICE;
use crate::{KeyError, Presence};

fn attributes(account: &str) -> [(&'static str, &str); 2] {
    [("application", SERVICE), ("ghostex-key", account)]
}

pub(crate) fn read_secret(account: &str) -> Result<Option<Vec<u8>>, KeyError> {
    let store = |error: oo7::Error| KeyError::KeyStore(error.to_string());
    futures::executor::block_on(async {
        let keyring = oo7::Keyring::new().await.map_err(store)?;
        keyring.unlock().await.map_err(store)?;
        for item in keyring
            .search_items(&attributes(account))
            .await
            .map_err(store)?
        {
            item.unlock().await.map_err(store)?;
            let secret = item.secret().await.map_err(store)?;
            if !secret.is_empty() {
                return Ok(Some(secret.to_vec()));
            }
        }
        Ok(None)
    })
}

pub(crate) fn write_secret(account: &str, secret: &[u8]) -> Result<(), KeyError> {
    let store = |error: oo7::Error| KeyError::KeyStore(error.to_string());
    futures::executor::block_on(async {
        let keyring = oo7::Keyring::new().await.map_err(store)?;
        keyring.unlock().await.map_err(store)?;
        keyring
            .create_item("Ghostex key", &attributes(account), secret, true)
            .await
            .map_err(store)
    })
}

/*
CDXC:Security 2026-11-08 WHY:
polkit asks for the system password. pkcheck checks an action for a subject and lets the desktop's polkit agent show its prompt; the subject is the short-lived pkcheck process itself (--process $$ under exec), so polkit's "keep" cache for org.freedesktop.policykit.exec never lets a second approval through without a prompt. A Ghostex-owned polkit action with its own message needs a policy file installed as root, which the AppImage can't do.
*/
pub(crate) fn confirm_presence(_reason: &str) -> Result<Presence, KeyError> {
    let status = Command::new("sh")
        .arg("-c")
        .arg("exec pkcheck --action-id org.freedesktop.policykit.exec --process $$ --allow-user-interaction")
        .status()
        .map_err(|error| KeyError::PresenceUnavailable(format!("pkcheck: {error}")))?;
    match status.code() {
        Some(0) => Ok(Presence::Polkit),
        // 1: not authorized, 2: the challenge was dismissed.
        Some(1) | Some(2) => Err(KeyError::Cancelled),
        Some(code) => Err(KeyError::PresenceUnavailable(format!(
            "no polkit agent answered (pkcheck exit {code})"
        ))),
        None => Err(KeyError::Cancelled),
    }
}
