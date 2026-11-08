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

/*
CDXC:RemotePairing 2026-11-08 DECISION:
User (GitHub #495, Q1721): a paired phone's SSH key must never be an unrestricted login. The
security review asked for a tunnel-only line (`command=`, `no-pty`, `permitopen`), but the phone
really uses its SSH login as a shell: its terminal tabs are interactive PTY shells (`openTerminal`,
`exec "$SHELL" -l`), every `ghostex` verb it runs (sessions, chat, files, the endpoint handshake)
is an exec through the login shell, uploads go over SFTP/SCP, and the web preview forwards to any
port and LAN host the computer can reach. A tunnel-only line would break all of that, so the line
keeps PTY, exec, SFTP and local forwarding and drops everything the phone never uses: `restrict`
turns off agent and X11 forwarding, `~/.ssh/rc` and every option a later OpenSSH adds, and
`pty,port-forwarding` give back the two the phone needs. Making it a tunnel needs the phone's
terminal and commands to move onto gxserver's own API first (reported on #495).
SEE-ALSO: apps/mobile/app/src/remote/commands.ts, apps/mobile/app/src/terminal/sessions.ts.
*/
pub const PAIRED_KEY_OPTIONS: &str = "restrict,pty,port-forwarding";

/// The `authorized_keys` line for a paired device:
/// `restrict,pty,port-forwarding <type> <blob> ghostex-paired:<deviceId> <deviceName>`.
pub fn authorized_key_line(key: &ParsedSshPublicKey, device_id: &str, device_name: &str) -> String {
    format!(
        "{PAIRED_KEY_OPTIONS} {} {} {} {}\n",
        key.key_type,
        key.blob_base64,
        authorized_key_marker(device_id),
        single_line_comment(device_name)
    )
}

/// Appends [`authorized_key_line`], creating
/// `~/.ssh` (0700) and `authorized_keys` (0600) when they do not exist yet.
/// On Windows the line goes wherever sshd reads this account's keys from,
/// through a UAC prompt when that is outside the profile (see
/// `windows_authorized_keys.rs`).
pub fn append_authorized_key(
    key: &ParsedSshPublicKey,
    device_id: &str,
    device_name: &str,
) -> Result<()> {
    let line = authorized_key_line(key, device_id, device_name);
    #[cfg(windows)]
    if let super::windows_authorized_keys::KeysFile::Elevated(path) =
        super::windows_authorized_keys::keys_file_for_this_account()?
    {
        return super::windows_authorized_keys::append_elevated(&path, &line);
    }
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
/// whitespace-separated field. Returns whether any line was removed. On
/// Windows a key that is not in the profile's file is looked for in the file
/// sshd reads for this account, through a UAC prompt when that is outside
/// the profile.
pub fn remove_authorized_key(device_id: &str) -> Result<bool> {
    let marker = authorized_key_marker(device_id);
    if remove_marked_lines_from_user_file(&marker)? {
        return Ok(true);
    }
    #[cfg(windows)]
    if let super::windows_authorized_keys::KeysFile::Elevated(path) =
        super::windows_authorized_keys::keys_file_for_this_account()?
    {
        return super::windows_authorized_keys::remove_elevated(&path, &marker);
    }
    Ok(false)
}

/// Removes the device's line without ever asking for administrator approval: `Ok(Some(_))` when
/// the file that holds this account's keys was checked (whether a line was there), `Ok(None)` when
/// that file needs a Windows administrator prompt, which only an explicit Remove may show.
pub fn remove_authorized_key_without_prompt(device_id: &str) -> Result<Option<bool>> {
    let removed = remove_marked_lines_from_user_file(&authorized_key_marker(device_id))?;
    #[cfg(windows)]
    if !removed
        && matches!(
            super::windows_authorized_keys::keys_file_for_this_account()?,
            super::windows_authorized_keys::KeysFile::Elevated(_)
        )
    {
        return Ok(None);
    }
    Ok(Some(removed))
}

fn remove_marked_lines_from_user_file(marker: &str) -> Result<bool> {
    let path = authorized_keys_path();
    remove_marked_lines_from_file(&path, marker)
}

/// [`remove_marked_lines_from_user_file`] on any file (tests use a temporary one).
pub(crate) fn remove_marked_lines_from_file(path: &std::path::Path, marker: &str) -> Result<bool> {
    let path = path.to_path_buf();
    if !path.is_file() {
        return Ok(false);
    }
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

/// A Windows administrator prompt the user declined. Pairing and unpairing
/// answer it with their own sentence instead of a generic failure.
#[derive(Debug)]
pub struct AdministratorApprovalDeclined {
    pub keys_file: PathBuf,
}

impl std::fmt::Display for AdministratorApprovalDeclined {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Windows asked for administrator approval to change {}, and the prompt was declined.",
            self.keys_file.display()
        )
    }
}

impl std::error::Error for AdministratorApprovalDeclined {}

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

#[cfg(test)]
mod tests {
    use super::*;

    const ED25519: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl phone";

    #[test]
    fn a_paired_key_line_is_restricted_and_keeps_only_pty_and_forwarding() {
        let key = parse_ssh_public_key(ED25519).unwrap();
        let line = authorized_key_line(&key, "d1", "Pixel\n8");
        assert_eq!(
            line,
            format!(
                "restrict,pty,port-forwarding ssh-ed25519 {} ghostex-paired:d1 Pixel 8\n",
                key.blob_base64
            )
        );
        let options = line.split_ascii_whitespace().next().unwrap();
        // No agent or X11 forwarding, no ~/.ssh/rc: `restrict` turns them off and nothing turns
        // them back on.
        for allowed in options.split(',') {
            assert!(["restrict", "pty", "port-forwarding"].contains(&allowed));
        }
        // The marker stays a field of its own, so removal still finds exactly this line.
        assert!(line
            .split_ascii_whitespace()
            .any(|field| field == authorized_key_marker("d1")));
    }

    #[test]
    fn removal_finds_a_restricted_line_and_leaves_other_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("authorized_keys");
        let key = parse_ssh_public_key(ED25519).unwrap();
        let own = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl me@laptop\n";
        let legacy = format!(
            "ssh-ed25519 {} ghostex-paired:old iPhone\n",
            key.blob_base64
        );
        fs::write(
            &path,
            format!("{own}{}{legacy}", authorized_key_line(&key, "d1", "Pixel")),
        )
        .unwrap();
        assert!(remove_marked_lines_from_file(&path, &authorized_key_marker("d1")).unwrap());
        assert!(remove_marked_lines_from_file(&path, &authorized_key_marker("old")).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), own);
        assert!(!remove_marked_lines_from_file(&path, &authorized_key_marker("d1")).unwrap());
    }
}
