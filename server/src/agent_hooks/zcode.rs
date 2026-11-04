use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use serde_json::Value;

use crate::{domain::DomainStateError, platform::shell::command_shell};

#[cfg(not(windows))]
use super::probing::normalize_gxserver_process_path;
use super::{config::HookPaths, probing::run_command_stdout_with_timeout};

/// CDXC:AgentHooks 2026-10-16 WHY:
/// ZCode CLI 3.14 reads its runtime settings, hooks included, from `~/.zcode/cli/setting.json`. At its first start it copies `config.json` there once ("the old file is never used as a runtime configuration fallback", its CONFIGURATION.md), so hooks Ghostex wrote to `config.json` after that copy, or on a computer whose ZCode only has `setting.json`, never ran: ZCode sessions lost their working and attention status (2026-10-16). Hooks go to `setting.json` when it exists or when there is no `config.json` either, and to `config.json` only for a ZCode that has not made the switch yet, whose first 3.14 start then carries them over.
pub(crate) fn zcode_hook_config_path(home: &Path) -> PathBuf {
    let cli = home.join(".zcode/cli");
    let setting = cli.join("setting.json");
    let legacy = cli.join("config.json");
    if setting.exists() || !legacy.exists() {
        setting
    } else {
        legacy
    }
}

/// The settings file ZCode reads its session database path from (`storage.sessionDbPath`).
pub(crate) fn zcode_settings_files(home: &Path) -> [PathBuf; 2] {
    let cli = home.join(".zcode/cli");
    [cli.join("setting.json"), cli.join("config.json")]
}

/// CDXC:AgentHooks 2026-09-15 WHY:
/// ZCode only creates its model defaults and setup marker when config.json does not exist.
/// Writing a hooks-only file first makes its runtime exit before /login is available.
/// Let the public launcher initialize its own defaults before merging Ghostex hooks.
pub(crate) fn ensure_zcode_config(
    path: &Path,
    hook_paths: &HookPaths,
) -> Result<(), DomainStateError> {
    ensure_initialized(path, || {
        // An install-on-demand wrapper would install ZCode just to write its defaults (on_demand.rs).
        if super::probing::resolve_probe_command("zcode", &hook_paths.home_dir).is_none() {
            return;
        }
        let shell = command_shell();
        let mut command = Command::new(&shell.executable);
        command.args(shell.profileless_script_args("zcode --help"));
        command
            .current_dir(&hook_paths.home_dir)
            .env("HOME", &hook_paths.home_dir)
            .env("USERPROFILE", &hook_paths.home_dir)
            .env("NO_UPDATE_NOTIFIER", "1")
            .env("ZCODE_DISABLE_UPDATE_CHECK", "1");
        #[cfg(not(windows))]
        command.env(
            "PATH",
            normalize_gxserver_process_path(
                std::env::var("PATH").ok().as_deref(),
                &hook_paths.home_dir,
            ),
        );
        let _ = run_command_stdout_with_timeout(command, Duration::from_secs(10));
    })
}

fn ensure_initialized(path: &Path, bootstrap: impl FnOnce()) -> Result<(), DomainStateError> {
    if !path.exists() {
        bootstrap();
    }
    // ZCode 3.14 keeps its model defaults in its provider file, so its `setting.json` only has to
    // be the settings file ZCode wrote; the older `config.json` must carry the model defaults.
    let settings_file = path.file_name().is_some_and(|name| name == "setting.json");
    let initialized = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .is_some_and(|config| {
            (settings_file && config.is_object())
                || config
                    .pointer("/model/main")
                    .and_then(Value::as_str)
                    .is_some_and(|model| !model.trim().is_empty())
        });
    if initialized {
        Ok(())
    } else {
        Err(DomainStateError::bad_request(format!(
            "ZCode's model defaults are missing from {}. Initialize or repair ZCode's configuration before installing hooks.",
            path.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstraps_missing_config_but_never_replaces_existing_settings() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.json");
        let defaults = r#"{"model":{"main":"zai/glm-5.2"},"provider":{"zai":{"options":{}}}}"#;
        ensure_initialized(&path, || fs::write(&path, defaults).unwrap()).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), defaults);
        ensure_initialized(&path, || panic!("existing config must not be bootstrapped")).unwrap();
        fs::write(&path, r#"{"hooks":{"enabled":true}}"#).unwrap();
        assert!(
            ensure_initialized(&path, || panic!("partial config must not be replaced")).is_err()
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            r#"{"hooks":{"enabled":true}}"#
        );
    }

    #[test]
    fn failed_bootstrap_does_not_create_a_hooks_only_config() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.json");
        assert!(ensure_initialized(&path, || {}).is_err());
        assert!(!path.exists());
    }
}
