use super::ProjectIcon;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::Path,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FolderIconKind {
    Home,
    Drive,
}

/// Tabler `home`, the house the desktop already ships (`apps/desktop/assets/titlebar/home.svg`).
const HOME_ICON_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#8a8a8a" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M19 8.71l-5.333 -4.148a2.7 2.7 0 0 0 -3.334 0l-5.333 4.148a2.7 2.7 0 0 0 -1 2.129v7.161a2 2 0 0 0 2 2h12a2 2 0 0 0 2 -2v-7.161a2.7 2.7 0 0 0 -1 -2.129z"/><path d="M16 20v-8h-8v8"/></svg>"##;

/// Lucide `hard-drive`: the Tabler set the app ships has no drive, and its `database` already means a SQLite file in Docs.
const DRIVE_ICON_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#8a8a8a" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 12H2"/><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/><path d="M6 16h.01"/><path d="M10 16h.01"/></svg>"##;

/// CDXC:Icons 2026-11-17 DECISION:
/// User: "the icon for my home folder (madda in this case or ~ on macos) and for drives like c:/ for example should be a home icon for my home folders on 3 oses and a drive icon on all 3 oses, i dont want to see a different icon that comes somehow from one of the apps below somehow this is wrong". A project whose folder is a home folder or a drive root never gets a favicon scanned from inside it (the nested monorepo scan found ~/dev/Kiwimake's kiwi for ~); gxserver publishes a house or a drive as its discovered icon instead, so every client, phones already installed included, draws it with no change of their own. An icon the user set by hand (`identityIcon`) still wins on every client.
pub(super) fn builtin_folder_icon(path: &Path) -> Option<ProjectIcon> {
    let home = ghostex_paths::user_home_dir();
    let kind =
        folder_icon_kind(&path.to_string_lossy(), &home.to_string_lossy()).or_else(|| {
            // A symlinked path (Silverblue's /home -> /var/home, a linked volume) is judged by where it points too.
            let canonical = std::fs::canonicalize(path).ok()?;
            let canonical_home = std::fs::canonicalize(&home).unwrap_or(home);
            folder_icon_kind(
                &canonical.to_string_lossy(),
                &canonical_home.to_string_lossy(),
            )
        })?;
    Some(folder_icon(kind))
}

fn folder_icon(kind: FolderIconKind) -> ProjectIcon {
    let (svg, source) = match kind {
        FolderIconKind::Home => (HOME_ICON_SVG, "builtin:home"),
        FolderIconKind::Drive => (DRIVE_ICON_SVG, "builtin:drive"),
    };
    let mut hasher = DefaultHasher::new();
    "image/svg+xml".hash(&mut hasher);
    svg.as_bytes().hash(&mut hasher);
    ProjectIcon {
        data_url: format!(
            "data:image/svg+xml;base64,{}",
            BASE64_STANDARD.encode(svg.as_bytes())
        ),
        content_hash: hasher.finish(),
        source_relative_path: source.to_string(),
    }
}

/// Splits a macOS, Linux, Windows or UNC path into its parts: `(drive, segments)`, where `drive` is
/// `Some("C:")` for a Windows path and `Some("//server/share")`'s server for a UNC one.
fn path_parts(path: &str) -> Option<(Option<String>, Vec<String>)> {
    let mut normalized = path.trim().replace('\\', "/");
    // Windows' verbatim prefix, what canonicalize returns there.
    if let Some(rest) = normalized.strip_prefix("//?/") {
        normalized = match rest.strip_prefix("UNC/") {
            Some(unc) => format!("//{unc}"),
            None => rest.to_string(),
        };
    }
    if normalized.is_empty() {
        return None;
    }
    let bytes = normalized.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        let drive = normalized[..2].to_ascii_uppercase();
        let segments = split_segments(&normalized[2..]);
        return Some((Some(drive), segments));
    }
    if let Some(unc) = normalized.strip_prefix("//") {
        let segments = split_segments(unc);
        let server = segments.first()?.clone();
        return Some((Some(format!("//{server}")), segments[1..].to_vec()));
    }
    if !normalized.starts_with('/') {
        return None;
    }
    Some((None, split_segments(&normalized)))
}

fn split_segments(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .map(str::to_string)
        .collect()
}

fn same_path(a: &(Option<String>, Vec<String>), b: &(Option<String>, Vec<String>)) -> bool {
    // Windows and macOS folders ignore case; a Linux home differing only in case is not a real case.
    a.0.as_deref().map(str::to_ascii_lowercase) == b.0.as_deref().map(str::to_ascii_lowercase)
        && a.1.len() == b.1.len()
        && a.1
            .iter()
            .zip(&b.1)
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
}

/// Folders under a `Users` directory that are not someone's home.
fn is_shared_users_folder(name: &str) -> bool {
    ["Shared", "Public", "Default", "Default User", "All Users"]
        .iter()
        .any(|shared| shared.eq_ignore_ascii_case(name))
}

pub(super) fn folder_icon_kind(path: &str, home: &str) -> Option<FolderIconKind> {
    let parts = path_parts(path)?;
    let home_parts =
        path_parts(home).filter(|(drive, segments)| drive.is_some() || !segments.is_empty());
    if home_parts
        .as_ref()
        .is_some_and(|home| same_path(&parts, home))
    {
        return Some(FolderIconKind::Home);
    }
    let user_name = home_parts
        .as_ref()
        .and_then(|(_, segments)| segments.last().cloned());
    let segments: Vec<&str> = parts.1.iter().map(String::as_str).collect();
    match (parts.0.as_deref(), segments.as_slice()) {
        // A Windows drive root (C:\) or a network share's root (\\server\share).
        (Some(drive), []) if !drive.starts_with("//") => Some(FolderIconKind::Drive),
        (Some(drive), [_share]) if drive.starts_with("//") => Some(FolderIconKind::Drive),
        // C:\Users\<user>.
        (Some(drive), [users, name])
            if !drive.starts_with("//")
                && users.eq_ignore_ascii_case("Users")
                && !is_shared_users_folder(name) =>
        {
            Some(FolderIconKind::Home)
        }
        (Some(_), _) => None,
        (None, []) => Some(FolderIconKind::Drive),
        // /Users/<user> (macOS), /home/<user> and /var/home/<user> (Linux), /mnt/c/Users/<user> (Windows' home seen from WSL).
        (None, ["Users", name]) if !is_shared_users_folder(name) && *name != "Guest" => {
            Some(FolderIconKind::Home)
        }
        (None, ["home", _]) | (None, ["var", "home", _]) => Some(FolderIconKind::Home),
        (None, ["mnt", drive, users, name])
            if drive.len() == 1
                && users.eq_ignore_ascii_case("Users")
                && !is_shared_users_folder(name) =>
        {
            Some(FolderIconKind::Home)
        }
        // Mounted volumes: /Volumes/<disk> (macOS), /mnt/<disk> (Linux, and WSL's /mnt/c), /media/<disk>,
        // /media/<user>/<disk> and /run/media/<user>/<disk>. /media/<user> alone is the folder those sit in.
        (None, ["Volumes", _]) | (None, ["mnt", _]) => Some(FolderIconKind::Drive),
        (None, ["media", name]) if user_name.as_deref() != Some(*name) => {
            Some(FolderIconKind::Drive)
        }
        (None, ["media", _, _]) | (None, ["run", "media", _, _]) => Some(FolderIconKind::Drive),
        _ => None,
    }
}
