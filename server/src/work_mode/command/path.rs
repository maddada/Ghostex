//! The program a workspace's ticket command names: the path the user supplied, resolved to an
//! absolute path. `~` is the home folder, an absolute path is kept, and a bare name
//! is looked up on the same PATH gxserver gives `gh`, so it finds what a terminal would.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The absolute path `typed` names. A relative path with a folder in it (`bin/source`) is refused:
/// gxserver's own folder is not one the user knows, so it would resolve somewhere unexpected.
pub(crate) fn resolve_command_path(typed: &str) -> Result<PathBuf, String> {
    resolve_command_path_in(
        typed,
        &ghostex_paths::GhostexPaths::resolve().home_dir,
        &crate::managed_tools::run::job_path(&[]),
    )
}

pub(crate) fn resolve_command_path_in(
    typed: &str,
    home: &Path,
    search_path: &OsString,
) -> Result<PathBuf, String> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Err("Set the path of the ticket command.".to_string());
    }
    if typed == "~" {
        return Ok(home.to_path_buf());
    }
    if let Some(rest) = typed
        .strip_prefix("~/")
        .or_else(|| typed.strip_prefix("~\\"))
    {
        return Ok(home.join(rest));
    }
    if typed.starts_with('~') {
        return Err(format!(
            "\"{typed}\": use ~/ for your home folder, an absolute path, or a command name on PATH."
        ));
    }
    let path = Path::new(typed);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    if typed.contains('/') || typed.contains('\\') {
        return Err(format!(
            "\"{typed}\" is relative: use an absolute path, ~/…, or a command name on PATH."
        ));
    }
    std::env::split_paths(search_path)
        .flat_map(|folder| candidates(&folder, typed))
        .find(|candidate| ghostex_paths::powershell::launchable(candidate))
        .ok_or_else(|| format!("Can't find \"{typed}\" on PATH."))
}

/// The files a bare name may be in one folder: the name itself, and on Windows with each
/// `PATHEXT` extension too.
fn candidates(folder: &Path, name: &str) -> Vec<PathBuf> {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut found = vec![folder.join(name)];
    #[cfg(windows)]
    {
        let extensions = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE".to_string());
        found.extend(
            extensions
                .split(';')
                .filter(|extension| !extension.is_empty())
                .map(|extension| folder.join(format!("{name}{extension}"))),
        );
    }
    found
}
