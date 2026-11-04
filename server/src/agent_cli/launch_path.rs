//! CDXC:AgentLauncher 2026-10-28 WHY:
//! User: "please fix this for dsh-tui and all the others". A DeepSeek Harness installed from Settings through mise could not start: its session ran a bare `dsh-tui` and the shell answered `env: dsh-tui: No such file or directory`. mise puts what it installs only in its own installs and shims folders, which a shell finds only after `mise activate` (or with the shims folder on PATH); gxserver's own probes look in the shims folder, so Settings said "installed" while every terminal said "not found".
//! A launch therefore runs an agent the session's login shell cannot find, but mise can, through mise itself: the absolute path of mise's shim on macOS and Linux (the shim is mise's own binary, which puts the tool and the runtime it needs, such as node, on PATH before it runs it), and `mise x -- <command>` on Windows, whose default shims are scripts that call `mise` by name. This works whatever PATH the session's shell builds, with no shell profile edits, and an agent the shell already finds is left exactly as configured. Putting the shims folder on the session's PATH instead was rejected: a shell profile that sets PATH outright (or a Linux terminal that skips the login profile) drops it again, and every plain terminal would gain every mise tool.
//! SEE-ALSO: server/src/agents/launch_plan.rs (`resolve_agent_launch_command`), server/src/zmx/session_glue.rs (plans made before the fix), server/src/agent_cli/mise.rs.

use std::path::{Path, PathBuf};

/// `command` with its program word replaced by what `resolve` answers for it. Only a bare program
/// name is offered to `resolve` (`dsh-tui`, not `./dsh-tui`, `/opt/bin/dsh-tui` or a quoted path), after
/// the environment assignments that may come first: `env NAME=value …` and `NAME=value …` in a POSIX
/// shell, `$env:NAME='value'; …` in PowerShell, and PowerShell's call operator `&`.
pub(crate) fn rewrite_program(
    command: &str,
    resolve: impl FnOnce(&str) -> Option<String>,
) -> String {
    let mut offset = 0;
    let mut in_powershell_assignment = false;
    while let Some((start, end, word)) = crate::agents::command_word(command, offset) {
        offset = end;
        if in_powershell_assignment {
            in_powershell_assignment = !word.ends_with(';');
            continue;
        }
        if word
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("$env:"))
        {
            in_powershell_assignment = !word.ends_with(';');
            continue;
        }
        if word == "env" || word == "&" || is_assignment(&word) {
            continue;
        }
        let raw = &command[start..end];
        let bare = raw == word
            && !word.is_empty()
            && word.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'+')
            })
            && !word.starts_with('.');
        if !bare {
            return command.to_string();
        }
        return match resolve(&word) {
            Some(program) => format!("{}{program}{}", &command[..start], &command[end..]),
            None => command.to_string(),
        };
    }
    command.to_string()
}

fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            && !name.as_bytes()[0].is_ascii_digit()
    })
}

/// mise's data folder: `MISE_DATA_DIR`, else `$XDG_DATA_HOME/mise`, else `~/.local/share/mise`
/// (`%LOCALAPPDATA%\mise` on Windows).
pub(crate) fn mise_data_dir(home: &Path) -> PathBuf {
    let set = |name: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    if let Some(dir) = set("MISE_DATA_DIR") {
        return dir;
    }
    if cfg!(windows) {
        if let Some(local) = set("LOCALAPPDATA") {
            return local.join("mise");
        }
    }
    set("XDG_DATA_HOME")
        .unwrap_or_else(|| home.join(".local").join("share"))
        .join("mise")
}

/// How a launch runs `name` through mise, or `None` when mise does not provide it.
/// `shim` is mise's shim for `name` when it exists, `mise` the mise binary.
pub(crate) fn mise_program(name: &str, shim: Option<&Path>, mise: Option<&str>) -> Option<String> {
    let quote = |path: &str| crate::agents::quote_shell_arg(path);
    if cfg!(windows) {
        // PowerShell runs a quoted path only through the call operator.
        return mise.map(|mise| format!("& {} x -- {name}", quote(mise)));
    }
    if let Some(shim) = shim {
        return Some(quote(&shim.to_string_lossy()));
    }
    mise.map(|mise| format!("{} x -- {name}", quote(mise)))
}

/// Whether the shell a session starts finds `name` on its own: on the login shell's PATH (the
/// registry PATH on Windows). An unreadable login PATH says no, so mise is asked.
fn session_shell_finds(name: &str, home: &Path) -> bool {
    #[cfg(windows)]
    let directories = {
        let _ = home;
        crate::platform::live_path::new_program_directories()
    };
    #[cfg(not(windows))]
    let directories: Vec<PathBuf> = crate::agent_hooks::probing::login_shell_path_patient(home)
        .into_iter()
        .map(PathBuf::from)
        // mise's shims count only when the shell itself put them there.
        .collect();
    crate::platform::live_path::find_in(name, &directories).is_some()
}

/// What runs `name` in a new session when its shell cannot find it but mise can.
fn mise_launch_program(name: &str, home: &Path) -> Option<String> {
    if session_shell_finds(name, home) {
        return None;
    }
    let mise = super::process::resolve("mise", home);
    let shim = mise_data_dir(home).join("shims").join(name);
    let shim = (!cfg!(windows) && shim.is_file()).then_some(shim);
    if shim.is_none() {
        // Only a tool mise really has: `mise which` fails for one it does not know.
        let mise = mise.as_deref()?;
        super::mise::which(mise, name, home)?;
    }
    mise_program(name, shim.as_deref(), mise.as_deref())
}

/// `command` with a program the session's shell cannot find replaced by mise's way of running it.
pub(crate) fn with_reachable_program(command: &str) -> String {
    // Unit tests assert exact commands, which must not depend on the CLIs installed on the machine.
    if cfg!(test) {
        return command.to_string();
    }
    let home = crate::resume_lookup::home_dir();
    rewrite_program(command, |name| mise_launch_program(name, &home))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shim(name: &str) -> Option<String> {
        (name == "dsh-tui").then(|| "'/home/u/.local/share/mise/shims/dsh-tui'".to_string())
    }

    #[test]
    fn a_bare_program_is_replaced_after_its_environment() {
        assert_eq!(
            rewrite_program("dsh-tui", shim),
            "'/home/u/.local/share/mise/shims/dsh-tui'"
        );
        assert_eq!(
            rewrite_program(
                "env DSH_TUI_NO_LAUNCHPAD=1 DSH_TUI_WORKSPACE_TARGET=. dsh-tui --resume abc",
                shim
            ),
            "env DSH_TUI_NO_LAUNCHPAD=1 DSH_TUI_WORKSPACE_TARGET=. '/home/u/.local/share/mise/shims/dsh-tui' --resume abc"
        );
        assert_eq!(
            rewrite_program("FOO=1 dsh-tui", shim),
            "FOO=1 '/home/u/.local/share/mise/shims/dsh-tui'"
        );
        assert_eq!(
            rewrite_program(
                "$env:DSH_TUI_NO_LAUNCHPAD='1'; $env:DSH_TUI_WORKSPACE_TARGET='.'; dsh-tui",
                shim
            ),
            "$env:DSH_TUI_NO_LAUNCHPAD='1'; $env:DSH_TUI_WORKSPACE_TARGET='.'; '/home/u/.local/share/mise/shims/dsh-tui'"
        );
    }

    #[test]
    fn paths_quoted_words_wrappers_and_unknown_programs_stay_as_written() {
        for command in [
            "/opt/bin/dsh-tui",
            "./dsh-tui",
            "'dsh-tui'",
            "codex --yolo",
            "cswap run 2 --share-history -- claude",
            "",
        ] {
            assert_eq!(rewrite_program(command, shim), command, "{command}");
        }
    }

    #[test]
    fn mise_runs_the_shim_or_execs_the_tool() {
        let shim_path = Path::new("/home/u/.local/share/mise/shims/gemini");
        if cfg!(windows) {
            assert_eq!(
                mise_program("gemini", None, Some(r"C:\tools\mise.exe")).as_deref(),
                Some(r"& 'C:\tools\mise.exe' x -- gemini")
            );
        } else {
            assert_eq!(
                mise_program("gemini", Some(shim_path), Some("/x/mise")).as_deref(),
                Some("'/home/u/.local/share/mise/shims/gemini'")
            );
            assert_eq!(
                mise_program("gemini", None, Some("/x/mise")).as_deref(),
                Some("'/x/mise' x -- gemini")
            );
        }
        assert_eq!(mise_program("gemini", None, None), None);
    }

    /// The clean-computer check, run by hand with a throwaway HOME and a PATH without mise or Node.js:
    /// `env -i HOME=<sandbox> PATH=/usr/bin:/bin:/usr/sbin:/sbin
    /// GHOSTEX_LIVE_MISE_SANDBOX=1 <test binary> --ignored --nocapture live_clean_install`. It installs
    /// DeepSeek Harness the way Settings > Agents does, updates it, and prints the command a new
    /// session would run.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn live_clean_install_then_launch() {
        if std::env::var_os("GHOSTEX_LIVE_MISE_SANDBOX").is_none() {
            return;
        }
        let paths = crate::paths::get_gxserver_paths(None);
        let home = paths.agent_config_home_dir().to_path_buf();
        assert!(
            home.to_string_lossy().contains("agent-work"),
            "run only with a throwaway HOME"
        );
        let before = crate::agent_cli::endpoint::read_agent(&home, "dsh")
            .await
            .unwrap();
        println!(
            "BEFORE executable={} methods={}",
            before["executablePath"],
            serde_json::to_string(&before["methods"]).unwrap()
        );
        let installed = crate::agent_cli::endpoint::install_default_and_wait(&paths, "dsh")
            .await
            .unwrap_or_else(|reason| panic!("install failed: {reason}"));
        println!(
            "INSTALLED executable={} version={} method={}",
            installed["executablePath"], installed["version"], installed["detectedMethodId"]
        );
        println!(
            "INSTALL OUTPUT\n{}",
            installed["job"]["output"].as_str().unwrap_or_default()
        );
        let updated = crate::agent_cli::endpoint::update_and_wait(&paths, "dsh", "mise")
            .await
            .unwrap_or_else(|reason| panic!("update failed: {reason}"));
        println!(
            "UPDATED version={} method={} command={}",
            updated["version"], updated["detectedMethodId"], updated["job"]["command"]
        );
        let launch = rewrite_program(&crate::agents::with_dsh_launch_env("dsh-tui"), |name| {
            mise_launch_program(name, &home)
        });
        println!("LAUNCH {launch}");
    }

    #[test]
    fn mise_keeps_its_data_under_home_by_default() {
        if std::env::var_os("MISE_DATA_DIR").is_none()
            && std::env::var_os("XDG_DATA_HOME").is_none()
            && !cfg!(windows)
        {
            assert_eq!(
                mise_data_dir(Path::new("/home/u")),
                PathBuf::from("/home/u/.local/share/mise")
            );
        }
    }
}
