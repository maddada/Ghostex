//! The folders agent CLIs keep their own config in, found the way each CLI finds them.

use std::path::{Path, PathBuf};

/*
CDXC:AgentHooks 2026-11-07 WHY:
Codex finds `~/.codex` with `dirs::home_dir()` (codex-rs/utils/home-dir `find_codex_home`, dirs 6), which on Windows is the known Profile folder (`SHGetKnownFolderPath(FOLDERID_Profile)`) and never reads HOME. Ghostex's home is HOME first, then USERPROFILE, so with a global HOME (Git Bash and MSYS set one) Ghostex installed Codex's hooks, wrote their trust and looked for its rollouts in a `.codex` Codex never opens (GitHub #458 follow-up). On macOS and Linux both read HOME, so nothing changes there.
*/
/// The folder Codex uses when CODEX_HOME is not set, for a Ghostex whose home is `home_dir`. An
/// isolated profile home (GHOSTEX_HOME, tests) keeps its own `.codex`.
pub fn codex_default_home(home_dir: &Path) -> PathBuf {
    codex_default_home_from(
        home_dir,
        &crate::user_home_dir(),
        cfg!(windows).then(windows_profile_dir).flatten(),
    )
}

/// `codex_default_home` with the user's home and Windows' Profile folder given, so the rule is
/// testable anywhere. `profile` is `None` off Windows.
pub fn codex_default_home_from(
    home_dir: &Path,
    user_home: &Path,
    profile: Option<PathBuf>,
) -> PathBuf {
    match profile {
        Some(profile) if home_dir == user_home => profile.join(".codex"),
        _ => home_dir.join(".codex"),
    }
}

/// The folder Codex uses: CODEX_HOME when it is set (and `respect_environment`, which an isolated
/// profile turns off), else `codex_default_home`.
pub fn codex_home(home_dir: &Path, respect_environment: bool) -> PathBuf {
    respect_environment
        .then(|| std::env::var_os("CODEX_HOME"))
        .flatten()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| codex_default_home(home_dir))
}

/*
CDXC:AgentHooks 2026-11-07 WHY:
Claude Code keeps `.claude` and `.claude.json` in `CLAUDE_CONFIG_DIR`, else in `os.homedir()` (its 2.1.306 bundle: `process.env.CLAUDE_CONFIG_DIR||join(homedir(),".claude")`; HOME is read only for git, gcloud and XDG config). It runs on Bun, whose `os.homedir()` on Windows is libuv's `uv_os_homedir`: USERPROFILE, else the account's profile folder, never HOME (bun src/runtime/node/node_os.rs, libuv src/win/util.c). Ghostex's HOME-first home put Claude's hooks, folder trust, logins and transcripts in a `.claude` Claude Code never opens whenever a global HOME was set.
*/
/// The folder Claude Code treats as home, for a Ghostex whose home is `home_dir`: on Windows
/// USERPROFILE, as Bun's `os.homedir()` reads it; an isolated profile home keeps its own.
pub fn claude_user_home(home_dir: &Path) -> PathBuf {
    claude_user_home_from(
        home_dir,
        &crate::user_home_dir(),
        cfg!(windows)
            .then(|| std::env::var_os("USERPROFILE"))
            .flatten()
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .filter(|path| path.is_absolute()),
    )
}

/// `claude_user_home` with the user's home and USERPROFILE given (`None` off Windows).
pub fn claude_user_home_from(
    home_dir: &Path,
    user_home: &Path,
    user_profile: Option<PathBuf>,
) -> PathBuf {
    match user_profile {
        Some(profile) if home_dir == user_home => profile,
        _ => home_dir.to_path_buf(),
    }
}

/// Claude Code's folder when CLAUDE_CONFIG_DIR is not set.
pub fn claude_default_home(home_dir: &Path) -> PathBuf {
    claude_user_home(home_dir).join(".claude")
}

/// Claude Code's `.claude.json` when CLAUDE_CONFIG_DIR is not set.
pub fn claude_json_path(home_dir: &Path) -> PathBuf {
    claude_user_home(home_dir).join(".claude.json")
}

/// Windows' known Profile folder, what `dirs::home_dir()` returns there.
#[cfg(windows)]
pub fn windows_profile_dir() -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_Profile, SHGetKnownFolderPath};

    let mut path = std::ptr::null_mut();
    // SAFETY: SHGetKnownFolderPath writes a CoTaskMemAlloc'd, NUL-terminated wide string to
    // `path` (null on failure); it is read once and freed with CoTaskMemFree as documented.
    unsafe {
        let result = SHGetKnownFolderPath(&FOLDERID_Profile, 0, std::ptr::null_mut(), &mut path);
        let folder = (result >= 0 && !path.is_null()).then(|| {
            let len = (0..).take_while(|&i| *path.add(i) != 0).count();
            PathBuf::from(std::ffi::OsString::from_wide(std::slice::from_raw_parts(
                path, len,
            )))
        });
        CoTaskMemFree(path as *const _);
        folder
    }
}

#[cfg(not(windows))]
pub fn windows_profile_dir() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_windows_home_from_home_uses_the_profile_folder_codex_uses() {
        // Git Bash's global HOME: Ghostex's home, while Codex reads the Profile folder.
        let ghostex_home = Path::new(r"D:\msys64\home\archana");
        assert_eq!(
            codex_default_home_from(
                ghostex_home,
                ghostex_home,
                Some(PathBuf::from(r"C:\Users\Archana"))
            ),
            PathBuf::from(r"C:\Users\Archana").join(".codex")
        );
    }

    #[test]
    fn an_isolated_home_keeps_its_own_codex_folder() {
        assert_eq!(
            codex_default_home_from(
                Path::new(r"C:\ghostex-test\home"),
                Path::new(r"C:\Users\Archana"),
                Some(PathBuf::from(r"C:\Users\Archana"))
            ),
            Path::new(r"C:\ghostex-test\home").join(".codex")
        );
    }

    #[test]
    fn a_windows_home_from_home_uses_userprofile_as_claude_code_does() {
        let ghostex_home = Path::new(r"D:\msys64\home\archana");
        assert_eq!(
            claude_user_home_from(
                ghostex_home,
                ghostex_home,
                Some(PathBuf::from(r"C:\Users\Archana"))
            ),
            PathBuf::from(r"C:\Users\Archana")
        );
        assert_eq!(
            claude_user_home_from(
                Path::new(r"C:\ghostex-test\home"),
                Path::new(r"C:\Users\Archana"),
                Some(PathBuf::from(r"C:\Users\Archana"))
            ),
            PathBuf::from(r"C:\ghostex-test\home"),
            "an isolated home keeps its own"
        );
        assert_eq!(
            claude_user_home_from(Path::new("/Users/a"), Path::new("/Users/a"), None),
            PathBuf::from("/Users/a")
        );
    }

    #[test]
    fn off_windows_codex_is_under_home() {
        assert_eq!(
            codex_default_home_from(Path::new("/Users/a"), Path::new("/Users/a"), None),
            Path::new("/Users/a/.codex")
        );
    }
}
