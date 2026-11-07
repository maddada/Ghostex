//! `ghostex box prepare-snapshot` / `finish-snapshot`: before Save as snapshot, take everything
//! that must not ride along out of the box's disk (plan §2.5): the secret files copied into
//! checkouts, Codex's login link, and the box's identity (its machine id, gxserver's identity and
//! token, and tailcat's key, which is the address every client dials). The identity files wait in
//! the private folder (tmpfs, which no snapshot saves) and come back afterwards; the secret files
//! and the login link are made again from `secrets.json`.
//!
//! A box made from the snapshot then has no identity on disk, so its gxserver and tailcat make
//! new ones at boot: two boxes from one snapshot never share an address or a token.

use std::path::{Path, PathBuf};

use ghostex_gx_protocol::cloud_box::BoxSnapshotPrepResult;

use super::{logins, prepare_session, state};

const STASH_DIR: &str = "snapshot-stash";

/// The identity files and their names in the stash.
fn identity_files() -> Vec<(PathBuf, &'static str)> {
    let paths = crate::paths::get_gxserver_paths(None);
    vec![
        (state::identity_file(), "box-identity.json"),
        (paths.identity_file.clone(), "gxserver-identity"),
        (paths.auth_token_file.clone(), "gxserver-auth-token"),
        (crate::tailcat::tailcat_key_file(&paths), "tailcat-key"),
        (
            crate::tailcat::tailcat_address_file(&paths),
            "tailcat-address",
        ),
    ]
}

fn move_file(from: &Path, to: &Path) -> Result<bool, String> {
    if !from.is_file() {
        return Ok(false);
    }
    // The stash is on tmpfs and the identity on disk, so a rename can't cross them.
    let bytes =
        std::fs::read(from).map_err(|error| format!("Can't read {}: {error}", from.display()))?;
    state::write_private(to, &bytes)?;
    std::fs::remove_file(from)
        .map_err(|error| format!("Can't remove {}: {error}", from.display()))?;
    Ok(true)
}

pub(crate) fn run_prepare_snapshot() -> Result<BoxSnapshotPrepResult, String> {
    let ticket = state::require_ticket()?;
    let stash = state::ensure_private_dir(&ticket.provider)?.join(STASH_DIR);
    let mut paths = Vec::new();
    for placed in state::read_placed().secret_files {
        if std::fs::remove_file(&placed.path).is_ok() {
            paths.push(placed.path);
        }
    }
    if let Some(link) = logins::remove_codex_login(&ticket.provider) {
        paths.push(link);
    }
    for (file, name) in identity_files() {
        if move_file(&file, &stash.join(name))? {
            paths.push(file.to_string_lossy().into_owned());
        }
    }
    Ok(BoxSnapshotPrepResult {
        paths,
        services_restarted: false,
    })
}

pub(crate) fn run_finish_snapshot() -> Result<BoxSnapshotPrepResult, String> {
    let ticket = state::require_ticket()?;
    let stash = state::private_dir(&ticket.provider).join(STASH_DIR);
    let mut paths = Vec::new();
    for (file, name) in identity_files() {
        if move_file(&stash.join(name), &file)? {
            paths.push(file.to_string_lossy().into_owned());
        }
    }
    let secrets = logins::read_secrets(&ticket.provider);
    let mut placed = state::read_placed();
    for checkout in placed.checkouts.clone() {
        let checkout = PathBuf::from(checkout);
        if checkout.is_dir() {
            paths.extend(prepare_session::place_secret_files(
                &checkout,
                &secrets,
                &mut placed,
            )?);
        }
    }
    state::write_placed(&placed)?;
    if let Some(link) = logins::restore_codex_login(&ticket.provider) {
        paths.push(link);
    }
    Ok(BoxSnapshotPrepResult {
        paths,
        services_restarted: false,
    })
}
