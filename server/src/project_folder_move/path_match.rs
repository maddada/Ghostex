//! Whether a stored path is the moved folder or inside it, and the same path under the new folder.
//!
//! Every comparison here is decided by facts read before the move (the folder's spellings and
//! whether its volume ignores letter case), because afterwards the old path names nothing on disk
//! (risk 4 of the plan). The part-by-part comparison is the shared one in
//! packages/paths/src/folder_below.rs (Rename Worktree and Locate Folder use it too); `MovedRoot`
//! keeps the spellings read before the move, which `MissingFolderRoot` can't once it has moved.

pub(crate) use ghostex_paths::folder_below::{
    is_windows_path, path_parts_below, split_path_parts as split_parts,
};

/// The moved folder as stored paths can spell it, and where it went.
#[derive(Clone, Debug)]
pub(crate) struct MovedRoot {
    /// Every spelling of the old folder: the project row's, the resolved one, each a full path.
    pub(crate) old_spellings: Vec<String>,
    pub(crate) new_path: String,
    /// The old folder's volume ignores letter case (always so for a Windows path).
    pub(crate) ignore_case: bool,
}

impl MovedRoot {
    /// The parts of `path` below the old folder (empty for the folder itself), or `None` when
    /// `path` is not the folder or inside it.
    pub(crate) fn rest_below<'a>(&self, path: &'a str) -> Option<Vec<&'a str>> {
        let path = path.trim();
        if path.is_empty() {
            return None;
        }
        for root in &self.old_spellings {
            let windows = cfg!(windows) || is_windows_path(root);
            let ignore_case = windows || self.ignore_case;
            // Both absolute, or both not: `/a/b` is not `a/b`.
            if path.starts_with('/') != root.starts_with('/') {
                continue;
            }
            if let Some(rest) = path_parts_below(path, root, windows, ignore_case) {
                return Some(rest);
            }
        }
        None
    }

    pub(crate) fn contains(&self, path: &str) -> bool {
        self.rest_below(path).is_some()
    }

    /// `path` under the new folder, or `None` when it is outside the old one. The result keeps
    /// the new folder's own separator.
    pub(crate) fn rebase(&self, path: &str) -> Option<String> {
        let rest = self.rest_below(path)?;
        if rest.is_empty() {
            return Some(self.new_path.clone());
        }
        let windows = cfg!(windows) || is_windows_path(&self.new_path);
        let separator = if windows { "\\" } else { "/" };
        let base = self
            .new_path
            .trim_end_matches(|c| c == '/' || (windows && c == '\\'));
        Some(format!("{base}{separator}{}", rest.join(separator)))
    }

    /// `value` with every string that is a path at or under the old folder moved under the new
    /// one, walking into objects and arrays. Only whole strings that are such paths change, never
    /// a path mentioned inside longer text. `None` when nothing changed.
    pub(crate) fn rebase_json(&self, value: &serde_json::Value) -> Option<serde_json::Value> {
        use serde_json::Value;
        match value {
            Value::String(text) => self.rebase(text).map(Value::String),
            Value::Array(items) => {
                let mut changed = false;
                let next = items
                    .iter()
                    .map(|item| match self.rebase_json(item) {
                        Some(moved) => {
                            changed = true;
                            moved
                        }
                        None => item.clone(),
                    })
                    .collect();
                changed.then_some(Value::Array(next))
            }
            Value::Object(map) => {
                let mut changed = false;
                let next = map
                    .iter()
                    .map(|(key, item)| match self.rebase_json(item) {
                        Some(moved) => {
                            changed = true;
                            (key.clone(), moved)
                        }
                        None => (key.clone(), item.clone()),
                    })
                    .collect();
                changed.then_some(Value::Object(next))
            }
            _ => None,
        }
    }

    /// A stored text column: JSON (an object or an array) is walked; anything else is a path or
    /// left alone. `None` when nothing changed.
    pub(crate) fn rebase_column(&self, text: &str) -> Option<String> {
        let trimmed = text.trim_start();
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            let value: serde_json::Value = serde_json::from_str(text).ok()?;
            return self
                .rebase_json(&value)
                .and_then(|moved| serde_json::to_string(&moved).ok());
        }
        self.rebase(text)
    }
}

#[cfg(test)]
mod tests {
    use super::MovedRoot;
    use serde_json::json;

    fn root(old: &str, new: &str, ignore_case: bool) -> MovedRoot {
        MovedRoot {
            old_spellings: vec![old.to_string()],
            new_path: new.to_string(),
            ignore_case,
        }
    }

    #[test]
    fn a_sibling_whose_name_starts_the_same_is_not_inside() {
        let moved = root("/a/App", "/a/Kiwi", false);
        assert!(moved.contains("/a/App"));
        assert!(moved.contains("/a/App/src"));
        assert!(!moved.contains("/a/App-feat"));
        assert!(!moved.contains("/a/Apple/src"));
        assert!(!moved.contains("/a"));
        assert!(
            !moved.contains("a/App/src"),
            "a relative path is never inside"
        );
        assert!(!moved.contains(""));
        assert_eq!(moved.rebase("/a/App").as_deref(), Some("/a/Kiwi"));
        assert_eq!(
            moved.rebase("/a/App/src/main.rs").as_deref(),
            Some("/a/Kiwi/src/main.rs")
        );
        assert_eq!(moved.rebase("/a/App-feat"), None);
    }

    #[test]
    fn letter_case_counts_only_on_a_volume_that_keeps_it() {
        let keeps_case = root("/a/App", "/a/Kiwi", false);
        assert!(!keeps_case.contains("/a/app/src"));
        let ignores_case = root("/a/App", "/a/Kiwi", true);
        assert_eq!(
            ignores_case.rebase("/A/app/src").as_deref(),
            Some("/a/Kiwi/src")
        );
    }

    #[test]
    fn a_windows_path_matches_either_separator_and_any_case() {
        let moved = root(r"C:\dev\App", r"C:\dev\Kiwi", false);
        assert_eq!(
            moved.rebase(r"c:\DEV\app\src\main.rs").as_deref(),
            Some(r"C:\dev\Kiwi\src\main.rs")
        );
        assert_eq!(
            moved.rebase("C:/dev/App/src").as_deref(),
            Some(r"C:\dev\Kiwi\src")
        );
        assert!(!moved.contains(r"C:\dev\App-feat"));
        assert!(!moved.contains(r"D:\dev\App"));
    }

    #[test]
    fn json_changes_whole_path_strings_only() {
        let moved = root("/a/App", "/a/Kiwi", false);
        let stored = json!({
            "cwd": "/a/App/tools",
            "note": "see /a/App/tools",
            "list": ["/a/App", "/b/Other"],
            "count": 1,
        });
        assert_eq!(
            moved.rebase_json(&stored),
            Some(json!({
                "cwd": "/a/Kiwi/tools",
                "note": "see /a/App/tools",
                "list": ["/a/Kiwi", "/b/Other"],
                "count": 1,
            }))
        );
        assert_eq!(moved.rebase_json(&json!({ "cwd": "/b/Other" })), None);
        assert_eq!(
            moved.rebase_column("/a/App/x").as_deref(),
            Some("/a/Kiwi/x")
        );
        assert_eq!(
            moved.rebase_column(r#"["/a/App"]"#).as_deref(),
            Some(r#"["/a/Kiwi"]"#)
        );
        assert_eq!(moved.rebase_column("plain text"), None);
    }
}
