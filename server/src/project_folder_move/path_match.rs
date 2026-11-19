//! Whether a stored path is the moved folder or inside it, and the same path under the new folder.
//!
//! Every comparison here is decided by facts read before the move (the folder's spellings and
//! whether its volume ignores letter case), because afterwards the old path names nothing on disk
//! (risk 4 of the plan; the same rule Rename Worktree follows since its Windows fix).

/// A path with a drive letter (`C:\…`, `C:/…`) or a UNC or verbatim prefix (`\\server\…`).
pub(crate) fn is_windows_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with(r"\\")
        || (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
}

/// A path's parts: split on `/`, and on `\` too in a Windows path, whose `\\?\` prefix is dropped.
pub(crate) fn split_parts(path: &str, windows: bool) -> Vec<&str> {
    let path = if windows {
        path.strip_prefix(r"\\?\").unwrap_or(path)
    } else {
        path
    };
    path.split(|c| c == '/' || (windows && c == '\\'))
        .filter(|part| !part.is_empty() && *part != ".")
        .collect()
}

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
            let path_parts = split_parts(path, windows);
            let root_parts = split_parts(root, windows);
            if root_parts.is_empty() || path_parts.len() < root_parts.len() {
                continue;
            }
            // Both absolute, or both not: `/a/b` is not `a/b`.
            if path.starts_with('/') != root.starts_with('/') {
                continue;
            }
            let same = path_parts.iter().zip(&root_parts).all(|(a, b)| {
                if ignore_case {
                    a.to_lowercase() == b.to_lowercase()
                } else {
                    a == b
                }
            });
            if same {
                return Some(path_parts[root_parts.len()..].to_vec());
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
