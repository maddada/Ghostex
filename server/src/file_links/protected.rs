//! Places a file link never reads, whatever folder it was made for: keys and credentials in the
//! home folder, Ghostex's own state and config (its token, the paired devices, the security log),
//! and, inside a folder link, hidden files and folders (`.env`, `.git`, `.ssh` under a project).
//!
//! CDXC:Security 2026-11-08 WHY: a link opens in a browser with no gxserver token (the token in its path is the only grant) and can be copied or forwarded, so a folder link for a project that is the home folder, or that holds a `.env` or `.git` folder, would have served those to whoever holds it. The design review (GitHub #495, section 5) asked that links never reach `~/.ssh` or Ghostex's state. This keeps the 2026-10-16 DECISION in registry.rs (pictures, video, audio and PDFs from any folder the session can read) and only takes these places out of it.
//! SEE-ALSO: server/src/file_links/serve.rs (`resolve` checks every request), registry.rs (minting refuses them too).

use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

/// Folders under the home folder that hold keys, tokens or credentials.
const HOME_SECRET_DIRS: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".aws",
    ".azure",
    ".kube",
    ".docker",
    ".password-store",
    ".config/gcloud",
    ".config/gh",
    ".config/op",
    "Library/Keychains",
    "Library/Cookies",
];

/// Files under the home folder that hold credentials.
const HOME_SECRET_FILES: &[&str] = &[".netrc", ".git-credentials", ".npmrc", ".pypirc", ".pgpass"];

/// Folder names that hold keys wherever they are.
const SECRET_DIR_NAMES: &[&str] = &[".ssh", ".gnupg", ".aws"];

fn protected_roots() -> &'static Vec<PathBuf> {
    static ROOTS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    ROOTS.get_or_init(|| {
        let paths = crate::paths::get_gxserver_paths(None);
        let mut roots = Vec::new();
        for dir in HOME_SECRET_DIRS.iter().chain(HOME_SECRET_FILES) {
            roots.push(paths.home_dir.join(dir));
        }
        // Ghostex's state (its token, paired devices, security log, databases) and config.
        roots.push(paths.app_state_dir.clone());
        roots.push(paths.app_config_dir.clone());
        roots.push(paths.root_dir.clone());
        // Compare real paths: the home folder may sit behind a symlink.
        roots
            .into_iter()
            .flat_map(|root| {
                let real = std::fs::canonicalize(&root).ok();
                std::iter::once(root).chain(real)
            })
            .collect()
    })
}

/// Whether `file` (a real, absolute path) is a place no link may read.
pub(super) fn is_protected(file: &Path) -> bool {
    is_protected_with(file, protected_roots())
}

fn is_protected_with(file: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| file.starts_with(root))
        || file.components().any(|component| match component {
            Component::Normal(name) => SECRET_DIR_NAMES
                .iter()
                .any(|secret| name == std::ffi::OsStr::new(secret)),
            _ => false,
        })
}

/// Whether a path inside a folder link names a hidden file or folder (`.env`, `.git/config`).
pub(super) fn names_hidden_entry(relative: &Path) -> bool {
    relative.components().any(|component| match component {
        Component::Normal(name) => name.to_string_lossy().starts_with('.'),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_ghostex_state_are_protected() {
        let roots = vec![
            PathBuf::from("/home/u/.ssh"),
            PathBuf::from("/home/u/.local/state/ghostex"),
        ];
        assert!(is_protected_with(
            Path::new("/home/u/.ssh/id_ed25519"),
            &roots
        ));
        assert!(is_protected_with(
            Path::new("/home/u/.local/state/ghostex/gxserver/auth/token"),
            &roots
        ));
        // A key folder anywhere, for a project that is a whole home folder elsewhere.
        assert!(is_protected_with(Path::new("/srv/box/.ssh/id_rsa"), &roots));
        assert!(!is_protected_with(
            Path::new("/home/u/code/app/shot.png"),
            &roots
        ));
        assert!(!is_protected_with(
            Path::new("/home/u/.local/share/ghostex/i/screenshot.png"),
            &roots
        ));
    }

    #[test]
    fn hidden_entries_inside_a_folder_link() {
        assert!(names_hidden_entry(Path::new(".env")));
        assert!(names_hidden_entry(Path::new(".git/config")));
        assert!(names_hidden_entry(Path::new("docs/.secret/notes.md")));
        assert!(!names_hidden_entry(Path::new("docs/index.html")));
    }

    #[test]
    fn the_real_home_secret_folders_are_listed() {
        let paths = crate::paths::get_gxserver_paths(None);
        assert!(is_protected(
            &paths.home_dir.join(".ssh").join("id_ed25519")
        ));
        assert!(is_protected(&paths.auth_token_file));
    }
}
