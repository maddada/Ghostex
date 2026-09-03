use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};
use base64::{
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
    Engine,
};
use sha2::{Digest, Sha256};

/*
CDXC:RemotePairing 2026-09-03:
Every key a paired device registers is written as one `authorized_keys` line
whose comment starts with `ghostex-paired:<deviceId>`. The marker is what
makes removal exact: unpairing deletes only the line whose marker field equals
the device id and leaves every hand-added key untouched, and a device name
can never collide with the marker because the id comes first.
*/
pub const AUTHORIZED_KEY_MARKER_PREFIX: &str = "ghostex-paired:";

/// An RSA-4096 line is under 1 KB; anything past this is not a key.
pub const SSH_PUBLIC_KEY_MAX_BYTES: usize = 8 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedSshPublicKey {
    pub key_type: String,
    pub blob_base64: String,
    pub blob: Vec<u8>,
}

impl ParsedSshPublicKey {
    /// OpenSSH's `SHA256:<base64 without padding>` fingerprint of the blob.
    pub fn fingerprint(&self) -> String {
        format!(
            "SHA256:{}",
            STANDARD_NO_PAD.encode(Sha256::digest(&self.blob))
        )
    }
}

/// Accepts one OpenSSH public key line (`<type> <base64> [comment]`) and
/// rejects anything else: multiple lines, unknown key types, undecodable
/// blobs, and blobs whose embedded type does not match the declared one.
pub fn parse_ssh_public_key(text: &str) -> Result<ParsedSshPublicKey> {
    if text.len() > SSH_PUBLIC_KEY_MAX_BYTES {
        bail!("sshPublicKey is larger than {SSH_PUBLIC_KEY_MAX_BYTES} bytes.");
    }
    let trimmed = text.trim();
    if trimmed.is_empty() {
        bail!("sshPublicKey is empty.");
    }
    if trimmed.contains('\n') || trimmed.contains('\r') {
        bail!("sshPublicKey must be a single OpenSSH public key line.");
    }
    let mut fields = trimmed.split_ascii_whitespace();
    let key_type = fields
        .next()
        .ok_or_else(|| anyhow!("sshPublicKey is missing the key type."))?;
    let blob_base64 = fields
        .next()
        .ok_or_else(|| anyhow!("sshPublicKey is missing the base64 key data."))?;
    if !is_supported_ssh_key_type(key_type) {
        bail!("sshPublicKey has an unsupported key type: {key_type}");
    }
    let blob = STANDARD
        .decode(blob_base64)
        .map_err(|error| anyhow!("sshPublicKey data is not valid base64: {error}"))?;
    if !blob_declares_key_type(&blob, key_type) {
        bail!("sshPublicKey data does not match its declared key type.");
    }
    Ok(ParsedSshPublicKey {
        key_type: key_type.to_string(),
        blob_base64: blob_base64.to_string(),
        blob,
    })
}

fn is_supported_ssh_key_type(key_type: &str) -> bool {
    key_type.starts_with("ssh-")
        || key_type.starts_with("ecdsa-sha2-")
        || key_type.starts_with("sk-ssh-")
        || key_type.starts_with("sk-ecdsa-")
}

/// The wire blob starts with a length-prefixed copy of the key type string.
fn blob_declares_key_type(blob: &[u8], key_type: &str) -> bool {
    if blob.len() < 4 {
        return false;
    }
    let length = u32::from_be_bytes([blob[0], blob[1], blob[2], blob[3]]) as usize;
    blob.len() >= 4 + length && &blob[4..4 + length] == key_type.as_bytes()
}

pub fn authorized_keys_path() -> PathBuf {
    ssh_directory_path().join("authorized_keys")
}

fn ssh_directory_path() -> PathBuf {
    crate::ghostex_cli::rpc::home_dir().join(".ssh")
}

pub fn authorized_key_marker(device_id: &str) -> String {
    format!("{AUTHORIZED_KEY_MARKER_PREFIX}{device_id}")
}

/// Appends `<type> <blob> ghostex-paired:<deviceId> <deviceName>`, creating
/// `~/.ssh` (0700) and `authorized_keys` (0600) when they do not exist yet.
pub fn append_authorized_key(
    key: &ParsedSshPublicKey,
    device_id: &str,
    device_name: &str,
) -> Result<()> {
    ensure_user_authorized_keys_grants_access()?;
    let directory = ssh_directory_path();
    if !directory.is_dir() {
        fs::create_dir_all(&directory)
            .with_context(|| format!("create {}", directory.display()))?;
        set_private_mode(&directory, 0o700)?;
    }
    let path = authorized_keys_path();
    let existed = path.is_file();
    let existing = if existed {
        fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?
    } else {
        String::new()
    };
    let line = format!(
        "{} {} {} {}\n",
        key.key_type,
        key.blob_base64,
        authorized_key_marker(device_id),
        single_line_comment(device_name)
    );
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("open {}", path.display()))?;
    if !existed {
        set_private_mode(&path, 0o600)?;
    }
    let prefix = if existing.is_empty() || existing.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    file.write_all(format!("{prefix}{line}").as_bytes())
        .with_context(|| format!("append to {}", path.display()))?;
    Ok(())
}

/// Deletes exactly the line(s) carrying `ghostex-paired:<deviceId>` as a
/// whitespace-separated field. Returns whether any line was removed.
pub fn remove_authorized_key(device_id: &str) -> Result<bool> {
    let path = authorized_keys_path();
    if !path.is_file() {
        return Ok(false);
    }
    let marker = authorized_key_marker(device_id);
    let existing = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    // Split with the terminators kept, so LF/CRLF endings and the presence or
    // absence of a final newline survive the rewrite byte for byte.
    let mut rewritten = String::with_capacity(existing.len());
    let mut removed = 0usize;
    for line in existing.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        if content
            .split_ascii_whitespace()
            .any(|field| field == marker)
        {
            removed += 1;
        } else {
            rewritten.push_str(line);
        }
    }
    if removed == 0 {
        return Ok(false);
    }
    let temp_path = path.with_extension("ghostex-tmp");
    fs::write(&temp_path, rewritten).with_context(|| format!("write {}", temp_path.display()))?;
    set_private_mode(&temp_path, 0o600)?;
    fs::rename(&temp_path, &path).with_context(|| format!("replace {}", path.display()))?;
    Ok(true)
}

/// Comments end at the newline, so the device name is flattened to one line
/// of printable characters before it goes into the file.
fn single_line_comment(device_name: &str) -> String {
    device_name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|character| !character.is_control())
        .collect()
}

/*
CDXC:RemotePairing 2026-09-03:
Known gap on Windows: OpenSSH Server ignores `%USERPROFILE%\.ssh\authorized_keys`
for members of the Administrators group and reads
`%ProgramData%\ssh\administrators_authorized_keys` instead (the default
`sshd_config` `Match Group administrators` block). Writing that file needs
elevation and ACL repair, which pairing does not do yet, so an administrator
account gets a clear failure instead of a key line that sshd will never read.
*/
#[cfg(windows)]
fn ensure_user_authorized_keys_grants_access() -> Result<()> {
    const ADMINISTRATORS_GROUP_SID: &str = "S-1-5-32-544";
    let output = std::process::Command::new("whoami")
        .args(["/groups", "/fo", "csv", "/nh"])
        .output()
        .with_context(|| "read group membership with whoami")?;
    let groups = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && groups.contains(ADMINISTRATORS_GROUP_SID) {
        bail!(
            "Pairing failed: this account is an administrator, and SSH on this computer reads administrator keys from {}\\ssh\\administrators_authorized_keys instead of the account's authorized_keys file. Add the key there by hand, or pair from a standard account.",
            std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".to_string())
        );
    }
    Ok(())
}

#[cfg(not(windows))]
fn ensure_user_authorized_keys_grants_access() -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_mode(path: &std::path::Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .with_context(|| format!("chmod {:o} {}", mode, path.display()))
}

#[cfg(not(unix))]
fn set_private_mode(_path: &std::path::Path, _mode: u32) -> Result<()> {
    Ok(())
}
