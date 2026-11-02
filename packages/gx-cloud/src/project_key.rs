//! The project key: what names a project across machines (the box's provider metadata, snapshots).

/// The repository's origin URL without scheme, user, `.git` or trailing slash, lowercased host:
/// `git@github.com:Org/Repo.git` and `https://github.com/Org/Repo` both give
/// `github.com/Org/Repo`. Without an origin, the caller uses the home project's path.
pub fn project_key_from_origin(origin: &str) -> Option<String> {
    let trimmed = origin.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let without_scheme = trimmed
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed);
    let without_user = without_scheme
        .rsplit_once('@')
        .map(|(_, rest)| rest)
        .unwrap_or(without_scheme);
    // scp-like `host:path` (no scheme) uses `:` before the path.
    let normalized = if !trimmed.contains("://") {
        without_user.replacen(':', "/", 1)
    } else {
        without_user.to_string()
    };
    let normalized = normalized.trim_end_matches(".git").trim_end_matches('/');
    let (host, path) = normalized.split_once('/')?;
    // Drop an explicit port from the host.
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    if host.is_empty() || path.is_empty() {
        return None;
    }
    Some(format!("{host}/{path}"))
}
