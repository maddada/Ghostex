//! Whether a folder path names a registered project's folder, compared the way the filesystem
//! compares them.
//!
//! CDXC:Projects 2026-10-31 WHY:
//! macOS volumes are usually case-insensitive and report a folder's on-disk capitals (`getcwd`, `canonicalize`), so a project added as `~/Dev/App` while the folder is `~/dev/app` never matched an agent's own folder, and `ghostex browser open` (and the other `--path` verbs) said "No matching project". Two paths that differ only in letter case are the same folder when both resolve to one folder on disk; on a case-sensitive volume (Linux, case-sensitive APFS) the miscapitalised one does not exist or is another folder, so nothing changes there. Symlinks are deliberately not followed for the match: a path that differs by more than case keeps matching only exactly, as before.

use std::path::Path;

/// A project path as compared: trimmed, trailing slashes removed (`/` stays `/`).
fn trimmed_path(path: &str) -> &str {
    let path = path.trim();
    let without = path.trim_end_matches('/');
    if without.is_empty() && path.starts_with('/') {
        "/"
    } else {
        without
    }
}

/// `a` and `b` name the same folder: the same text, or text that differs only in letter case and
/// resolves to the same folder on disk.
pub fn same_folder(a: &str, b: &str) -> bool {
    let (a, b) = (trimmed_path(a), trimmed_path(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    a == b || same_folder_ignoring_case_on_disk(a, b)
}

fn same_folder_ignoring_case_on_disk(a: &str, b: &str) -> bool {
    if a.to_lowercase() != b.to_lowercase() {
        return false;
    }
    match (
        std::fs::canonicalize(Path::new(a)),
        std::fs::canonicalize(Path::new(b)),
    ) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// The first candidate whose path is `wanted`: an exact text match wins over one that matches
/// only through letter case on disk, so a project registered with the exact path keeps winning.
pub fn find_same_folder<T>(
    wanted: &str,
    candidates: impl IntoIterator<Item = (T, String)>,
) -> Option<T> {
    let wanted = trimmed_path(wanted);
    if wanted.is_empty() {
        return None;
    }
    let mut by_case = None;
    for (item, path) in candidates {
        let path = trimmed_path(&path);
        if path == wanted {
            return Some(item);
        }
        if by_case.is_none() && same_folder_ignoring_case_on_disk(path, wanted) {
            by_case = Some(item);
        }
    }
    by_case
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh folder under the system temp dir with `name` inside it, removed on drop.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "ghostex-same-folder-{}-{}",
                std::process::id(),
                name
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(root.join(name)).unwrap();
            Scratch(root)
        }

        fn path(&self, tail: &str) -> String {
            self.0.join(tail).to_string_lossy().to_string()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Whether the volume `root` is on ignores letter case.
    fn volume_ignores_case(root: &Path) -> bool {
        let probe = root.join("CaseProbe");
        std::fs::create_dir_all(&probe).unwrap();
        let ignores = root.join("caseprobe").exists();
        let _ = std::fs::remove_dir_all(&probe);
        ignores
    }

    #[test]
    fn the_same_text_is_the_same_folder_with_or_without_a_trailing_slash() {
        assert!(same_folder("/no/such/App", "/no/such/App/"));
        assert!(!same_folder("/no/such/App", "/no/such/Other"));
        assert!(!same_folder("", ""));
    }

    #[test]
    fn different_capitals_match_only_where_the_volume_says_they_are_one_folder() {
        let scratch = Scratch::new("MyProject");
        let registered = scratch.path("MyProject");
        let typed_differently = scratch.path("myproject");
        // On a case-insensitive volume (macOS by default) they are one folder; on a case-sensitive
        // one (Linux) `myproject` does not exist and stays a different path.
        assert_eq!(
            same_folder(&registered, &typed_differently),
            volume_ignores_case(&scratch.0)
        );
    }

    #[test]
    fn missing_folders_that_differ_in_case_never_match() {
        assert!(!same_folder("/no/such/App", "/no/such/app"));
    }

    #[test]
    fn an_exact_registration_wins_over_a_case_only_one() {
        let scratch = Scratch::new("Repo");
        let exact = scratch.path("Repo");
        let other_case = scratch.path("REPO");
        let picked = find_same_folder(
            &exact,
            [("other case", other_case.clone()), ("exact", exact.clone())],
        );
        assert_eq!(picked, Some("exact"));
        // With only the miscapitalised registration, it is found where the volume ignores case.
        let picked = find_same_folder(&exact, [("other case", other_case)]);
        assert_eq!(picked.is_some(), volume_ignores_case(&scratch.0));
    }
}
