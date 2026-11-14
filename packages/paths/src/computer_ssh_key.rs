//! Where this computer keeps the SSH key it signs in to other computers with, once it paired with
//! them through Easy Connect.
//!
//! CDXC:RemotePairing 2026-11-14 SEE-ALSO: gxserver makes the key and sends its public half when
//! it pairs with another computer (server/src/remote_access/computer_ssh_key.rs,
//! computer_pairing.rs); the desktop dials that computer with it before any saved password
//! (apps/desktop/src/app/helpers/remote/config.rs). Both read the path from here.

use std::path::{Path, PathBuf};

/// The private key's file under gxserver's state folder; `.pub` beside it holds the public line.
pub fn computer_ssh_key_file(gxserver_state_dir: &Path) -> PathBuf {
    gxserver_state_dir.join("pairing").join("computer-ssh-key")
}
