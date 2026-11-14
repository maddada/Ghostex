//! This computer's own SSH key for signing in to the computers it pairs with through Easy Connect.
//!
//! CDXC:RemotePairing 2026-11-14 DECISION:
//! User, on the Easy Connect page (Q3 "Key first, password fallback"): "computers connected with
//! Easy Connect sign in with a key from pairing, like the phone does. When the key fails (for
//! example the other computer runs an older Ghostex), fall back to today's saved username and
//! password." gxserver makes one ed25519 key per computer here (`ssh-keygen`, the OpenSSH client's
//! own tool, so `ssh -i` reads it everywhere) and sends its public line with every computer pairing
//! (`computer_pairing.rs`); a computer that knows the field writes it as a `ghostex-paired:` line,
//! like a phone's, and says so in its answer (`sshKeyInstalled`). An older one ignores the field,
//! and this computer keeps signing in with the saved password there.
//! SEE-ALSO: packages/paths/src/computer_ssh_key.rs (the path), server/src/remote_access/pair_device.rs
//! (the other computer's side), apps/desktop/src/app/helpers/remote/config.rs (dials with it).

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};

use crate::paths::GxserverPaths;

use super::authorized_keys::parse_ssh_public_key;

/// The private key's path (`.pub` beside it).
pub fn computer_ssh_key_path(paths: &GxserverPaths) -> PathBuf {
    ghostex_paths::computer_ssh_key::computer_ssh_key_file(&paths.root_dir)
}

fn public_key_path(private_key: &Path) -> PathBuf {
    let mut name = private_key.as_os_str().to_owned();
    name.push(".pub");
    PathBuf::from(name)
}

/// The key's public line (`ssh-ed25519 AAAA… ghostex-computer`), making the key first when there
/// is none (or only half of one).
pub fn load_or_create_computer_ssh_public_key(paths: &GxserverPaths) -> Result<String> {
    let private_key = computer_ssh_key_path(paths);
    let public_key = public_key_path(&private_key);
    if private_key.is_file() {
        if let Some(line) = read_public_line(&public_key) {
            return Ok(line);
        }
    }
    // Half a key (a crash between the two files) is made again whole.
    let _ = std::fs::remove_file(&private_key);
    let _ = std::fs::remove_file(&public_key);
    if let Some(parent) = private_key.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let output = Command::new(ssh_keygen())
        .args([
            "-q",
            "-t",
            "ed25519",
            "-N",
            "",
            "-C",
            "ghostex-computer",
            "-f",
        ])
        .arg(&private_key)
        .output()
        .context("run ssh-keygen")?;
    if !output.status.success() {
        bail!(
            "ssh-keygen could not make this computer's key: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // ssh-keygen writes 0600 already; the folder stays the user's own as well.
        let _ = std::fs::set_permissions(&private_key, std::fs::Permissions::from_mode(0o600));
    }
    read_public_line(&public_key).ok_or_else(|| anyhow!("ssh-keygen made no readable public key"))
}

fn read_public_line(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let line = text.lines().next()?.trim().to_string();
    parse_ssh_public_key(&line).ok()?;
    Some(line)
}

/// `ssh-keygen` from the system's OpenSSH (on Windows, the one in System32 when PATH lacks it).
fn ssh_keygen() -> PathBuf {
    #[cfg(windows)]
    {
        let system = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
            .join(r"System32\OpenSSH\ssh-keygen.exe");
        if system.is_file() {
            return system;
        }
    }
    PathBuf::from("ssh-keygen")
}
