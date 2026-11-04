//! A Git review: the request a commit, push or pull request is confirmed under, and the check that
//! a confirmed selection only names files that review listed.

use super::state::{GitChangedFile, GitReviewAction};

/// A remote project a review belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitRemoteProject {
    pub machine_id: String,
    pub project_id: String,
}

/// `GpuiPendingGitCommitRequest`: what a review request id stands for until the dialog confirms,
/// cancels or starts Multiple Commits.
///
/// CDXC:Git 2026-06-24-15:22:
/// GPUI commit review stores the gxserver-derived changed-file list with the request id. Later
/// modal selections and diff clicks may only reference those paths, so CEF cannot stage or inspect
/// arbitrary renderer-supplied paths. Treat the modal's all-selected case as that stored review
/// list instead of a fresh unbounded add-all, so files created after review opens cannot slip into
/// the confirmed commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingGitReview {
    pub action: GitReviewAction,
    pub files: Vec<GitChangedFile>,
    pub has_commit: bool,
    /// The project on this computer, or the remote project on its machine.
    pub project_id: String,
    pub remote: Option<GitRemoteProject>,
}

impl PendingGitReview {
    pub fn has_file(&self, path: &str) -> bool {
        self.files.iter().any(|file| file.path == path)
    }

    pub fn file(&self, path: &str) -> Option<&GitChangedFile> {
        self.files.iter().find(|file| file.path == path)
    }
}

/// `GpuiTrustedGitReviewFileSelection`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustedFileSelection {
    /// The dialog named its files (rather than leaving every file selected).
    pub explicit: bool,
    pub file_paths: Vec<String>,
}

/// `resolveTrustedGitReviewFileSelection`: the files a confirm may commit. Every path must be one
/// the review listed; no path at all is an error.
pub fn resolve_trusted_file_selection(
    review: &PendingGitReview,
    file_paths: Option<&[String]>,
) -> Result<TrustedFileSelection, &'static str> {
    let explicit = file_paths.is_some();
    let candidates: Vec<String> = match file_paths {
        Some(paths) => paths.to_vec(),
        None => review.files.iter().map(|file| file.path.clone()).collect(),
    };
    let mut selected: Vec<String> = Vec::new();
    for candidate in candidates {
        let trusted = normalize_relative_git_file_path(&candidate)
            .filter(|path| review.has_file(path))
            .ok_or("Selected file is not part of the current Git review.")?;
        if !selected.contains(&trusted) {
            selected.push(trusted);
        }
    }
    if selected.is_empty() {
        return Err("Select at least one changed file.");
    }
    Ok(TrustedFileSelection {
        explicit,
        file_paths: selected,
    })
}

/// `normalizeGpuiRelativeGitFilePath`: a project-relative path with forward slashes, or `None`
/// for anything that could leave the project.
pub fn normalize_relative_git_file_path(file_path: &str) -> Option<String> {
    let replaced = file_path.replace('\\', "/");
    let normalized = replaced.trim_start_matches('/').trim();
    if normalized.is_empty() || normalized.contains('\0') {
        return None;
    }
    if normalized
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return None;
    }
    Some(normalized.to_string())
}

/// `parseGpuiSidebarGitCommitMessage`: the first line is the subject, the rest the body.
pub fn parse_commit_message(message: &str) -> (String, String) {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return (String::new(), String::new());
    }
    let mut lines = trimmed
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line));
    let subject = lines.next().unwrap_or_default().trim().to_string();
    let body = lines.collect::<Vec<_>>().join("\n").trim().to_string();
    (subject, body)
}
