//! How the Windows statusLine command starts the user's own status line command.
//!
//! CDXC:AgentProviders 2026-11-04 WHY:
//! Claude Code refreshes the status line several times a second while it works, and each refresh ran the user's command as `<Git>\bin\bash.exe -c <command>`. That `bin\bash.exe` is Git for Windows' launcher, which starts `usr\bin\bash.exe` in turn, and an npm-installed command such as `ccstatusline` then went through its `sh` shim before node: five processes (seven with conhosts) per refresh and about 23 refreshes a minute across the user's sessions (measured 2026-11-04). The real `usr\bin\bash.exe` is started directly, with the environment the launcher would have given it, and a command that is one word naming a program (an `.exe`, or the `.cmd` shim npm writes for a node script) starts that program directly with no shell at all. Anything else still runs in bash, which is the shell Claude Code itself runs the command in.
//! SEE-ALSO: `run_native_statusline_hook` in statusline.rs.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// The command that runs `wrapped` the way Claude Code would run it in Git Bash, or in PowerShell when Git for Windows is not installed.
pub(super) fn wrapped_command(wrapped: &str) -> Command {
    if let Some(command) = direct_program(wrapped) {
        return command;
    }
    if let Some((root, bash)) = git_bash() {
        let mut command = Command::new(bash);
        command.arg("-c").arg(wrapped);
        launcher_environment(&mut command, &root);
        return command;
    }
    // PowerShell 7 when installed anywhere, else 5.1: the shell sessions use (ghostex_paths::powershell).
    let mut command = Command::new(crate::platform::shell::powershell_executable());
    command
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
        .arg(wrapped);
    command
}

/// Git for Windows' folder and its real `usr\bin\bash.exe`, found from the `git` on PATH (`<root>\cmd\git.exe` or `<root>\mingw64\bin\git.exe`).
fn git_bash() -> Option<(PathBuf, PathBuf)> {
    let git = crate::platform::live_path::find("git", &[])?;
    git.ancestors().skip(2).take(2).find_map(|root| {
        let bash = root.join("usr").join("bin").join("bash.exe");
        bash.is_file().then(|| (root.to_path_buf(), bash))
    })
}

/// What Git for Windows' `bin\bash.exe` launcher sets before it starts `usr\bin\bash.exe`: the MINGW64 environment, its tool folders first on PATH, and a HOME.
fn launcher_environment(command: &mut Command, root: &Path) {
    if std::env::var_os("MSYSTEM").is_none() {
        command.env("MSYSTEM", "MINGW64");
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut folders: Vec<PathBuf> = std::env::split_paths(&path).collect();
    for folder in [
        root.join("usr").join("bin"),
        root.join("mingw64").join("bin"),
    ] {
        if !folders
            .iter()
            .any(|existing| same_folder(existing, &folder))
        {
            folders.insert(0, folder);
        }
    }
    if let Ok(path) = std::env::join_paths(folders) {
        command.env("PATH", path);
    }
    if std::env::var_os("HOME").is_none() {
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            command.env("HOME", profile);
        }
    }
}

fn same_folder(left: &Path, right: &Path) -> bool {
    let normal = |path: &Path| {
        path.to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .replace('/', "\\")
            .to_ascii_lowercase()
    };
    normal(left) == normal(right)
}

/// `wrapped` started without a shell, when it is a single word naming a program on PATH: an executable as it is, or an npm `.cmd` shim as node and its script.
fn direct_program(wrapped: &str) -> Option<Command> {
    let word = wrapped.trim();
    if word.is_empty()
        || !word
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '@'))
    {
        return None;
    }
    let found = crate::platform::live_path::find(word, &[])?;
    let extension = found
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("exe" | "com") => Some(Command::new(found)),
        Some("cmd") => {
            let shim = std::fs::read_to_string(&found).ok()?;
            let directory = found.parent()?;
            let script = directory.join(npm_shim_script(&shim)?);
            if !script.is_file() {
                return None;
            }
            let node = Some(directory.join("node.exe"))
                .filter(|node| node.is_file())
                .or_else(|| crate::platform::live_path::find("node", &[]))?;
            let mut command = Command::new(node);
            command.arg(script);
            Some(command)
        }
        _ => None,
    }
}

/// The script an npm `.cmd` shim (cmd-shim) hands to node, relative to the shim's folder: the last quoted `%dp0%\…` (or older `%~dp0\…`) word before `%*` on its launch line, when it names a JavaScript file.
fn npm_shim_script(shim: &str) -> Option<PathBuf> {
    let line = shim
        .lines()
        .rev()
        .find(|line| line.trim_end().ends_with("%*"))?;
    let before_arguments = line.trim_end().strip_suffix("%*")?.trim_end();
    let quoted = before_arguments.strip_suffix('"')?;
    let start = quoted.rfind('"')?;
    let word = &quoted[start + 1..];
    let relative = word
        .strip_prefix("%dp0%\\")
        .or_else(|| word.strip_prefix("%~dp0\\"))?;
    let is_script = Path::new(relative)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "js" | "mjs" | "cjs"
            )
        });
    (is_script && !relative.contains("..")).then(|| PathBuf::from(relative))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_script_of_a_current_npm_shim() {
        let shim = "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL :find_dp0\r\n\r\nIF EXIST \"%dp0%\\node.exe\" (\r\n  SET \"_prog=%dp0%\\node.exe\"\r\n) ELSE (\r\n  SET \"_prog=node\"\r\n  SET PATHEXT=%PATHEXT:;.JS;=;%\r\n)\r\n\r\nendLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & \"%_prog%\"  \"%dp0%\\node_modules\\ccstatusline\\dist\\ccstatusline.js\" %*\r\n";
        assert_eq!(
            npm_shim_script(shim),
            Some(PathBuf::from(
                r"node_modules\ccstatusline\dist\ccstatusline.js"
            ))
        );
    }

    #[test]
    fn reads_the_script_of_an_older_npm_shim() {
        let shim = "@IF EXIST \"%~dp0\\node.exe\" (\r\n  \"%~dp0\\node.exe\"  \"%~dp0\\node_modules\\tool\\bin\\tool.mjs\" %*\r\n) ELSE (\r\n  @SETLOCAL\r\n  node  \"%~dp0\\node_modules\\tool\\bin\\tool.mjs\" %*\r\n)\r\n";
        assert_eq!(
            npm_shim_script(shim),
            Some(PathBuf::from(r"node_modules\tool\bin\tool.mjs"))
        );
    }

    #[test]
    fn a_shim_that_is_not_npm_s_runs_in_the_shell() {
        assert_eq!(npm_shim_script("@echo off\r\ncall other.bat %*\r\n"), None);
        assert_eq!(
            npm_shim_script("\"%dp0%\\..\\outside\\tool.js\" %*\r\n"),
            None
        );
    }

    #[test]
    fn only_a_single_plain_word_skips_the_shell() {
        for command in [
            "npx -y ccstatusline@latest",
            "ccstatusline | head",
            "a;b",
            "$HOME/tool",
        ] {
            assert!(direct_program(command).is_none(), "{command}");
        }
    }
}
