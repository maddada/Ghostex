//! The git executable gxserver's own read-only Git polls start.
//!
//! CDXC:Git 2026-11-04 WHY:
//! On Windows the `git` on PATH is Git for Windows' `<Git>\cmd\git.exe`, a launcher that starts `<Git>\mingw64\bin\git.exe` and waits for it, so every poll was two process starts (and Defender scans) for one git command (measured 2026-11-03). The launcher's only job is to put Git's tool folders on PATH for what git itself starts (hooks, credential helpers, ssh), which reads such as `diff --numstat` and `rev-parse` never do, so those start the real git directly. Commands that may run hooks or ask for credentials keep the launcher. macOS and Linux have no launcher and keep `git`.

/// The program for a read-only git command: the real git behind Git for Windows' launcher when that is what PATH finds, otherwise `git`.
pub(crate) fn git_for_reads() -> String {
    #[cfg(windows)]
    {
        // Found once: Git for Windows does not move while gxserver runs, and a later install is picked up by the next search.
        static FOUND: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        if let Some(found) = FOUND.get() {
            return found.clone();
        }
        if let Some(real) = crate::platform::live_path::find("git", &[]).and_then(|git| {
            let launcher_folder = git.parent()?;
            if !launcher_folder
                .file_name()?
                .to_string_lossy()
                .eq_ignore_ascii_case("cmd")
            {
                return None;
            }
            let real = launcher_folder
                .parent()?
                .join("mingw64")
                .join("bin")
                .join("git.exe");
            real.is_file().then_some(real)
        }) {
            return FOUND
                .get_or_init(|| real.to_string_lossy().into_owned())
                .clone();
        }
    }
    "git".to_string()
}
