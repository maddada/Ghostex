use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use super::MANAGE_DOCS_CHAT_FILE_MOUNT_SEGMENT;
use crate::shared_settings;

/// CDXC:Docs 2026-09-16 WHY:
/// Open files and drafts survive reloads, so their native file grants must too.
/// A single temporary grant was cleared by Code links and replaced by the next Docs link, invalidating open documents and aliasing same-named files in different folders.
/// Each project/file pair has a stable mount backed by a native-owned record; the renderer stores only its Docs address.
pub(crate) struct ManageChatFileAuthorization {
    pub(crate) file_name: String,
    pub(crate) project_id: String,
    pub(crate) root: PathBuf,
}

fn authorization_directory() -> PathBuf {
    shared_settings::ghostex_storage_paths()
        .state_dir
        .join("docs-chat-files")
}

impl ManageChatFileAuthorization {
    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "fileName": self.file_name,
            "projectId": self.project_id,
            "root": self.root,
        })
    }

    fn id(&self) -> String {
        hex::encode(Sha256::digest(self.json().to_string().as_bytes()))
    }

    /// The grant a file at `address` (its real absolute path) would have in `project_id`.
    fn for_address(project_id: &str, address: &Path) -> Option<Self> {
        let address = simplified_path(address);
        Some(Self {
            file_name: address.file_name()?.to_str()?.to_string(),
            project_id: project_id.to_string(),
            root: address.parent()?.to_path_buf(),
        })
    }

    /// The same grant as `for_address` saved it before 2026-10-08, when the root still carried the
    /// Windows `\\?\` prefix `fs::canonicalize` adds; `None` where that prefix cannot apply.
    fn legacy_verbatim(&self) -> Option<Self> {
        let root = verbatim_windows_path(self.root.to_str()?)?;
        Some(Self {
            file_name: self.file_name.clone(),
            project_id: self.project_id.clone(),
            root: PathBuf::from(root),
        })
    }
}

/// `path` without the Windows verbatim prefix (`\\?\C:\x` becomes `C:\x`, `\\?\UNC\host\share`
/// becomes `\\host\share`) when it is representable without it; every other path comes back as is.
///
/// CDXC:Docs 2026-10-08 WHY: `fs::canonicalize` on Windows returns the verbatim form, which leaked
/// into the Open Files list as `\\?\C:\…` and made the same file addressable under two keys. Every
/// outside-file address is produced here without it. `\\?\` is dropped only for a drive or UNC path
/// shorter than MAX_PATH (the cases `dunce` also accepts), where Win32 treats both forms alike.
pub(crate) fn strip_verbatim_prefix(path: &str) -> String {
    const MAX_PATH: usize = 260;
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        let simple = format!(r"\\{rest}");
        if simple.len() < MAX_PATH {
            return simple;
        }
    } else if let Some(rest) = path.strip_prefix(r"\\?\") {
        let bytes = rest.as_bytes();
        let drive_path = bytes.len() >= 2
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes.get(2).is_none_or(|byte| *byte == b'\\');
        if drive_path && rest.len() < MAX_PATH {
            return rest.to_string();
        }
    }
    path.to_string()
}

fn simplified_path(path: &Path) -> PathBuf {
    match path.to_str() {
        Some(text) => PathBuf::from(strip_verbatim_prefix(text)),
        None => path.to_path_buf(),
    }
}

/// The verbatim spelling of a drive or UNC path, `None` for any other (or an already verbatim) one.
fn verbatim_windows_path(path: &str) -> Option<String> {
    if path.starts_with(r"\\?\") {
        return None;
    }
    if let Some(rest) = path.strip_prefix(r"\\") {
        return Some(format!(r"\\?\UNC\{rest}"));
    }
    let bytes = path.as_bytes();
    (bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\')
        .then(|| format!(r"\\?\{path}"))
}

/// `fs::canonicalize` with the Windows verbatim prefix removed (see `strip_verbatim_prefix`).
pub(crate) fn canonicalize_simplified(path: &Path) -> std::io::Result<PathBuf> {
    fs::canonicalize(path).map(|resolved| simplified_path(&resolved))
}

/// Grants `file`, which lies outside the project, to the project's Files view and returns its
/// Files address: the file's real absolute path.
///
/// CDXC:Docs 2026-09-30 DECISION:
/// User: Files never hands out the `.ghostex-chat-file/<id>/` routing address. A file outside the project is addressed, shown, copied and sent by its real absolute path; the hashed mount survives only inside the resource URLs its page and images load from (`manage_chat_file_resource_address`). Files from other projects may still open in the current project.
pub(crate) fn authorize_manage_chat_file(project_id: &str, file: &Path) -> Result<String, String> {
    let file = canonicalize_simplified(file)
        .map_err(|_| "That document is no longer available.".to_string())?;
    let address = file
        .to_str()
        .ok_or_else(|| "That document has no valid file path.".to_string())?
        .to_string();
    let authorization = ManageChatFileAuthorization::for_address(project_id, &file)
        .ok_or_else(|| "That document has no valid file name.".to_string())?;
    let id = authorization.id();
    let directory = authorization_directory();
    let destination = directory.join(format!("{id}.json"));
    let bytes = authorization.json().to_string().into_bytes();
    if fs::read(&destination).ok().as_deref() != Some(bytes.as_slice()) {
        let persist = || -> std::io::Result<()> {
            fs::create_dir_all(&directory)?;
            let temporary = directory.join(format!("{id}.{}.tmp", std::process::id()));
            let mut options = fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut output = options.open(&temporary)?;
            output.write_all(&bytes)?;
            output.sync_all()?;
            drop(output);
            fs::rename(&temporary, &destination)
        };
        persist().map_err(|error| format!("Could not remember this file for Files: {error}"))?;
    }
    Ok(address)
}

/// Whether a Files address names a file outside the project (by its absolute path).
pub(crate) fn manage_chat_file_is_address(path: &str) -> bool {
    Path::new(path).is_absolute()
}

/// The resource-origin path an outside file's page and images load from, `.ghostex-chat-file/<id>/<name>`,
/// so sibling stylesheets and images resolve inside its granted folder. Never shown to anyone.
pub(crate) fn manage_chat_file_resource_address(project_id: &str, address: &str) -> Option<String> {
    let authorization = ManageChatFileAuthorization::for_address(project_id, Path::new(address))?;
    let record_exists = |id: &str| {
        authorization_directory()
            .join(format!("{id}.json"))
            .is_file()
    };
    // A file opened before 2026-10-08 keeps the mount it was granted under (its `\\?\` root).
    let id = Some(authorization.id())
        .filter(|id| record_exists(id))
        .or_else(|| {
            authorization
                .legacy_verbatim()
                .map(|legacy| legacy.id())
                .filter(|id| record_exists(id))
        })
        .unwrap_or_else(|| authorization.id());
    Some(format!(
        "{MANAGE_DOCS_CHAT_FILE_MOUNT_SEGMENT}/{id}/{}",
        authorization.file_name
    ))
}

/// Splits the stable grant identity from a path within its document folder.
pub(crate) fn manage_chat_file_address(path: &str) -> Option<(&str, &str)> {
    let path = path
        .strip_prefix(MANAGE_DOCS_CHAT_FILE_MOUNT_SEGMENT)?
        .strip_prefix('/')?;
    let (id, inner) = path.split_once('/')?;
    (id.len() == 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
    .then_some((id, inner))
}

/// The grant behind an outside file's absolute address, or behind a resource-origin path inside
/// its granted folder; `None` when `project_id` holds no such grant.
pub(crate) fn resolve_manage_chat_file(
    project_id: &str,
    path: &str,
) -> Option<ManageChatFileAuthorization> {
    let ids = match manage_chat_file_address(path) {
        Some((id, _)) => vec![id.to_string()],
        None if manage_chat_file_is_address(path) => {
            let authorization =
                ManageChatFileAuthorization::for_address(project_id, Path::new(path))?;
            // A file opened before 2026-10-08 was granted under its `\\?\` root.
            let legacy = authorization.legacy_verbatim().map(|legacy| legacy.id());
            std::iter::once(authorization.id()).chain(legacy).collect()
        }
        None => return None,
    };
    let (id, bytes) = ids.into_iter().find_map(|id| {
        let bytes = fs::read(authorization_directory().join(format!("{id}.json"))).ok()?;
        Some((id, bytes))
    })?;
    let record: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let mut authorization = ManageChatFileAuthorization {
        file_name: record.get("fileName")?.as_str()?.to_string(),
        project_id: record.get("projectId")?.as_str()?.to_string(),
        root: PathBuf::from(record.get("root")?.as_str()?),
    };
    if authorization.project_id != project_id || authorization.id() != id {
        return None;
    }
    // A grant saved before 2026-10-08 hashes (and so must be checked) under its `\\?\` root, but
    // routes under the plain one every outside-file address now uses.
    authorization.root = simplified_path(&authorization.root);
    Some(authorization)
}

/// The real absolute path a resource-origin path names (a link inside an outside HTML file, or an
/// address saved before outside files were addressed by their real path).
pub(crate) fn manage_chat_file_real_path(project_id: &str, path: &str) -> Option<String> {
    let (_, inner) = manage_chat_file_address(path)?;
    let authorization = resolve_manage_chat_file(project_id, path)?;
    inner
        .split('/')
        .fold(authorization.root, |path, component| path.join(component))
        .to_str()
        .map(strip_verbatim_prefix)
}
