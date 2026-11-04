//! Which repository a project is, so the same checkout on two machines reads as one project.

/// A project as the Unified sidebar names and groups it.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepoIdentity {
    /// What two projects share when they are the same repository: the normalized origin
    /// (`github.com/owner/repo`), else `dir:` and the checkout's folder name.
    pub key: String,
    /// What a row and the filter show: the repository's name as its origin spells it, else the
    /// folder name.
    pub label: String,
}

/// The repository a project is.
///
/// CDXC:Sidebar 2026-10-30 DECISION:
/// User: "we should make the same repo on different machines somehow refer to the same project when we're in unified (repo name is used not the project name basically)". The Unified sidebar names a project by its git origin, which every gxserver already probes and publishes as `gitRemoteOriginUrl` (server/src/project_git_remote.rs publishes it raw, so normalizing is the client's job), and falls back to the checkout's folder name when there is no origin. A worktree project shares its parent's origin; without one it takes the parent's folder.
pub fn repo_identity(
    origin_url: Option<&str>,
    path: &str,
    parent_path: Option<&str>,
    fallback_title: &str,
) -> RepoIdentity {
    if let Some((key, label)) = origin_url.and_then(normalize_git_remote) {
        return RepoIdentity { key, label };
    }
    let folder = folder_name(parent_path.unwrap_or(path))
        .or_else(|| folder_name(path))
        .unwrap_or_else(|| fallback_title.trim().to_string());
    RepoIdentity {
        key: format!("dir:{}", folder.to_lowercase()),
        label: folder,
    }
}

/// `host/owner/repo` (lowercase) and the repository's name, for every form git prints an origin
/// in: `https://host/owner/repo(.git)`, `ssh://user@host:port/owner/repo`, `git@host:owner/repo`
/// and a local path. `None` for an empty value.
pub fn normalize_git_remote(url: &str) -> Option<(String, String)> {
    let url = url.trim().trim_end_matches('/');
    if url.is_empty() {
        return None;
    }
    let without_scheme = match url.split_once("://") {
        Some((_, rest)) => rest,
        None => url,
    };
    let (host, path) = match url.split_once("://") {
        // `scheme://[user@]host[:port]/path`
        Some(_) => {
            let (authority, path) = without_scheme
                .split_once('/')
                .unwrap_or((without_scheme, ""));
            let host = authority.rsplit('@').next().unwrap_or(authority);
            let host = host.split(':').next().unwrap_or(host);
            (host.to_string(), path.to_string())
        }
        None => match without_scheme.split_once(':') {
            // scp form `[user@]host:path`, but not a Windows drive (`C:\…`).
            Some((authority, path)) if authority.len() > 1 && !authority.contains('/') => {
                let host = authority.rsplit('@').next().unwrap_or(authority);
                (host.to_string(), path.to_string())
            }
            _ => (String::new(), without_scheme.to_string()),
        },
    };
    let path = path
        .trim_matches('/')
        .trim_end_matches(".git")
        .trim_end_matches('/')
        .replace('\\', "/");
    let label = path.rsplit('/').find(|part| !part.is_empty())?.to_string();
    let key = if host.is_empty() {
        path.to_lowercase()
    } else {
        format!("{}/{}", host.to_lowercase(), path.to_lowercase())
    };
    Some((key, label))
}

fn folder_name(path: &str) -> Option<String> {
    path.trim()
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .map(str::to_string)
}

/// Each repository key a list holds, mapped to the identity its row goes under, so one project on
/// several computers is one row however each computer spells or knows its origin.
///
/// CDXC:Sidebar 2026-11-04 DECISION:
/// User (Q1224, randomsand's report: "Projects with the same name on 2 computers sometimes merge into one row and sometimes show as two"): a project is one row with its computers on it, and it stays one row. The row key was the origin's `host/owner/repo`, else `dir:<folder>`, so the same repository split whenever one computer spelled its origin with an SSH host alias (`git@github-work:owner/repo`), had no `origin` remote, or hadn't been probed for one yet (gxserver publishes the origin a moment after the project, so a remote project could sit on its own row until then and join later). Now origins that name the same `owner/repo` are one repository whatever their host (the key with `github.com`, else the first, wins), and a project with no known origin joins the one repository whose name is its folder name. Two repositories that are provably different (different `owner/repo`, like a fork) stay two rows, and the row says `owner/repo` under its name (`repo_disambiguator`).
pub fn merge_repo_identities<'a>(
    identities: impl IntoIterator<Item = &'a RepoIdentity>,
) -> std::collections::BTreeMap<String, RepoIdentity> {
    use std::collections::BTreeMap;
    let mut origins: BTreeMap<String, Vec<&RepoIdentity>> = BTreeMap::new();
    let mut folders: Vec<&RepoIdentity> = Vec::new();
    for identity in identities {
        if identity.key.starts_with("dir:") {
            folders.push(identity);
        } else {
            origins
                .entry(owner_repo(&identity.key))
                .or_default()
                .push(identity);
        }
    }
    let mut merged: BTreeMap<String, RepoIdentity> = BTreeMap::new();
    let mut canonical_by_name: BTreeMap<String, Vec<RepoIdentity>> = BTreeMap::new();
    for group in origins.values() {
        let canonical = group
            .iter()
            .copied()
            .min_by_key(|identity| {
                (
                    !identity.key.starts_with("github.com/"),
                    identity.key.clone(),
                )
            })
            .cloned()
            .unwrap_or_default();
        for identity in group {
            merged.insert(identity.key.clone(), canonical.clone());
        }
        let name = canonical
            .key
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_string();
        let entry = canonical_by_name.entry(name).or_default();
        if !entry.contains(&canonical) {
            entry.push(canonical);
        }
    }
    for folder in folders {
        let name = folder.key.trim_start_matches("dir:").to_string();
        let target = match canonical_by_name.get(&name).map(Vec::as_slice) {
            Some([only]) => only.clone(),
            _ => folder.clone(),
        };
        merged.insert(folder.key.clone(), target);
    }
    merged
}

/// The identity one project of one computer goes under: its merged repository, except that a
/// folder with no origin stays its own when that computer already has the repository it would
/// join (`origin_keys_here`, the merged keys of that computer's projects with an origin), since
/// then it is another checkout rather than the same project.
pub fn resolve_repo_identity(
    identity: &RepoIdentity,
    merged: &std::collections::BTreeMap<String, RepoIdentity>,
    origin_keys_here: &std::collections::BTreeSet<String>,
) -> RepoIdentity {
    let target = merged.get(&identity.key).unwrap_or(identity);
    if identity.key.starts_with("dir:")
        && target.key != identity.key
        && origin_keys_here.contains(&target.key)
    {
        return identity.clone();
    }
    target.clone()
}

/// The merged keys of one computer's projects that have an origin.
pub fn origin_keys_of<'a>(
    identities: impl IntoIterator<Item = &'a RepoIdentity>,
    merged: &std::collections::BTreeMap<String, RepoIdentity>,
) -> std::collections::BTreeSet<String> {
    identities
        .into_iter()
        .filter(|identity| !identity.key.starts_with("dir:"))
        .map(|identity| {
            merged
                .get(&identity.key)
                .map_or_else(|| identity.key.clone(), |target| target.key.clone())
        })
        .collect()
}

/// The `owner/repo` part of an origin key (the last two segments), which is what two spellings of
/// one origin share.
fn owner_repo(key: &str) -> String {
    let parts: Vec<&str> = key.split('/').filter(|part| !part.is_empty()).collect();
    match parts.as_slice() {
        [.., owner, repo] => format!("{owner}/{repo}"),
        [repo] => (*repo).to_string(),
        [] => key.to_string(),
    }
}

#[cfg(test)]
mod merge_tests {
    use super::*;

    fn id(origin: Option<&str>, path: &str) -> RepoIdentity {
        repo_identity(origin, path, None, "")
    }

    #[test]
    fn one_repository_is_one_row_however_its_origin_is_spelled_or_known() {
        let https = id(Some("https://github.com/Owner/Ghostex.git"), "/a/Ghostex");
        let alias = id(Some("git@github-work:owner/ghostex.git"), "/b/Ghostex");
        let none = id(None, "/c/Ghostex");
        let fork = id(Some("git@github.com:fork/ghostex.git"), "/d/ghostex");
        let other = id(None, "/e/notes");
        let merged = merge_repo_identities([&https, &alias, &none, &fork, &other]);
        assert_eq!(merged[&alias.key].key, https.key);
        // A fork is a different repository, so a folder named like both stays on its own.
        assert_eq!(merged[&none.key].key, none.key);
        assert_eq!(merged[&fork.key].key, fork.key);
        assert_eq!(merged[&other.key].key, other.key);
        // Without the fork, the folder joins the one repository of its name.
        let merged = merge_repo_identities([&https, &alias, &none]);
        assert_eq!(merged[&none.key].key, https.key);
        assert_eq!(merged[&none.key].label, "Ghostex");
        // On another computer it joins the row; on the one that already has the repository it
        // is another checkout and keeps its own.
        let elsewhere = origin_keys_of([&none], &merged);
        assert_eq!(
            resolve_repo_identity(&none, &merged, &elsewhere).key,
            https.key
        );
        let here = origin_keys_of([&https, &none], &merged);
        assert_eq!(resolve_repo_identity(&none, &merged, &here).key, none.key);
    }
}
