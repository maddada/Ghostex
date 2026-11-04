use std::{
    env,
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
};

use super::*;
use serde_json::{Map, Value};

pub fn ghostty_terminal_config_backed_setting_keys_changed(
    previous_object: &Map<String, Value>,
    next_object: &Map<String, Value>,
) -> Vec<&'static str> {
    let previous_values = SharedGhosttyTerminalConfigValues::from_settings_object(previous_object);
    let next_values = SharedGhosttyTerminalConfigValues::from_settings_object(next_object);
    let mut keys = Vec::new();
    if previous_values.adjust_cell_height_percent != next_values.adjust_cell_height_percent {
        keys.push("adjust-cell-height");
    }
    if previous_values.adjust_cell_width != next_values.adjust_cell_width {
        keys.push("adjust-cell-width");
    }
    if previous_values.clipboard_paste_protection != next_values.clipboard_paste_protection {
        keys.push("clipboard-paste-protection");
    }
    if previous_values.clipboard_trim_trailing_spaces != next_values.clipboard_trim_trailing_spaces
    {
        keys.push("clipboard-trim-trailing-spaces");
    }
    if previous_values.confirm_close_surface != next_values.confirm_close_surface {
        keys.push("confirm-close-surface");
    }
    if previous_values.copy_on_select != next_values.copy_on_select {
        keys.push("copy-on-select");
    }
    if previous_values.cursor_style != next_values.cursor_style {
        keys.push("cursor-style");
    }
    if previous_values.cursor_style_blink != next_values.cursor_style_blink {
        keys.push("cursor-style-blink");
    }
    if previous_values.font_family != next_values.font_family {
        keys.push("font-family");
    }
    if previous_values.font_size != next_values.font_size {
        keys.push("font-size");
    }
    if previous_values.font_variation_weight != next_values.font_variation_weight {
        keys.push("font-variation");
    }
    if previous_values.ghostty_theme != next_values.ghostty_theme {
        keys.push("theme");
    }
    if previous_values.mouse_hide_while_typing != next_values.mouse_hide_while_typing {
        keys.push("mouse-hide-while-typing");
    }
    if previous_values.mouse_scroll_multiplier_discrete
        != next_values.mouse_scroll_multiplier_discrete
        || previous_values.mouse_scroll_multiplier_precision
            != next_values.mouse_scroll_multiplier_precision
    {
        keys.push("mouse-scroll-multiplier");
    }
    if previous_values.scrollback_limit_bytes != next_values.scrollback_limit_bytes {
        keys.push("scrollback-limit");
    }
    if previous_values.scrollbar != next_values.scrollbar {
        keys.push("scrollbar");
    }
    keys
}

pub fn write_ghostty_terminal_config_from_settings_object(
    object: &Map<String, Value>,
    changed_keys: &[&str],
) -> Result<SharedGhosttyConfigFileWriteStatus, SharedGhosttyConfigFileError> {
    let values = SharedGhosttyTerminalConfigValues::from_settings_object(object);
    merge_ghostex_terminal_config_file(|existing_config| {
        merge_ghostty_terminal_settings(existing_config, &values, changed_keys)
    })
}

pub fn apply_recommended_ghostty_config_file()
-> Result<SharedGhosttyConfigFileWriteStatus, SharedGhosttyConfigFileError> {
    merge_ghostex_terminal_config_file(|existing_config| {
        merge_ghostty_config_lines(existing_config, GHOSTEX_RECOMMENDED_GHOSTTY_CONFIG_LINES)
    })
}

pub fn reset_ghostty_config_file_to_defaults()
-> Result<SharedGhosttyConfigFileWriteStatus, SharedGhosttyConfigFileError> {
    merge_ghostex_terminal_config_file(|existing_config| {
        merge_ghostty_config_lines(existing_config, &[])
    })
}

/// Replaces every line of `keys` in the managed block of Ghostex's terminal config with `lines`.
pub(crate) fn merge_ghostex_terminal_config_lines(
    keys: &[&str],
    lines: Vec<String>,
) -> Result<SharedGhosttyConfigFileWriteStatus, SharedGhosttyConfigFileError> {
    merge_ghostex_terminal_config_file(|existing_config| {
        merge_ghostty_managed_config_block_entries(existing_config, keys, lines)
    })
}

/// The Ghostty app's own config, opened from Settings only to look at or edit by hand. Ghostex
/// never creates or writes it, so a missing file is reported instead of created.
pub fn prepare_ghostty_config_file_for_open() -> Result<PathBuf, SharedGhosttyConfigFileError> {
    let path = selected_ghostty_config_path()?;
    fs::metadata(&path)?;
    Ok(path)
}

pub fn prepare_ghostex_terminal_config_file_for_open()
-> Result<PathBuf, SharedGhosttyConfigFileError> {
    let path = ghostex_terminal_config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::metadata(&path) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            atomic_write_file(&path, GHOSTEX_TERMINAL_CONFIG_HEADER.as_bytes())?;
        }
        Err(error) => return Err(error.into()),
    }
    Ok(path)
}

/// CDXC:Terminal 2026-10-14 DECISION:
/// User: "I want to completely separate our Ghostty config in this app from the one the regular Ghostty uses (we just import fonts and a few things like that from Ghostty itself, but not the theme for example). And we should fully stop messing with users' Ghostty outside of Ghostex: no more section at the bottom of the Ghostty config for Ghostex. Don't delete it if it's already there, but migrate from that config to our own without changing things for users who already have this Ghostex config area."
/// Ghostex's terminals load only this file and every Settings write lands here; the Ghostty app's config is read only by the one-time migration (ghostty_config_migration.rs) and by Import from Ghostty. Supersedes the 2026-06-24 shared-config design, which wrote a marked block into the Ghostty app's config and loaded that file.
pub(crate) fn ghostex_terminal_config_path() -> PathBuf {
    ghostex_storage_paths().terminal_ghostty_config_file()
}

pub(crate) const GHOSTEX_TERMINAL_CONFIG_HEADER: &str = "\
# Ghostex terminal config (Ghostty syntax: https://ghostty.org/docs/config)
#
# Ghostex's terminals read only this file; the Ghostty app's own config is never
# read or changed. Settings > Terminal rewrites the lines between the
# \"Ghostex managed terminal settings\" markers; anything else here is yours.
";

fn merge_ghostex_terminal_config_file<F>(
    merge: F,
) -> Result<SharedGhosttyConfigFileWriteStatus, SharedGhosttyConfigFileError>
where
    F: FnOnce(&str) -> String,
{
    let path = ghostex_terminal_config_path();
    let (existing_config, base_config) = match fs::read_to_string(&path) {
        Ok(config) => (config.clone(), config),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            (String::new(), GHOSTEX_TERMINAL_CONFIG_HEADER.to_string())
        }
        Err(error) => return Err(error.into()),
    };
    let merged_config = merge(&base_config);
    if existing_config == merged_config {
        return Ok(SharedGhosttyConfigFileWriteStatus::Unchanged);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_write_file(&path, merged_config.as_bytes())?;
    Ok(SharedGhosttyConfigFileWriteStatus::Changed)
}

/// The Ghostty app's config path (read only: the migration, Import from Ghostty, Open Ghostty config).
pub(crate) fn selected_ghostty_config_path() -> Result<PathBuf, SharedGhosttyConfigFileError> {
    selected_ghostty_config_path_from_home(user_home_dir())
        .ok_or(SharedGhosttyConfigFileError::HomeUnavailable)
}

/// CDXC:PlatformSupport 2026-09-28 WHY:
/// Windows sets USERPROFILE, not HOME, so every settings save that touched a terminal value showed "Could not update Ghostty config" during first-run setup, and Ghostty themes were never found. HOME still wins where it is set (Git Bash, WSL-style shells).
pub(crate) fn user_home_dir() -> Option<OsString> {
    env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .or_else(|| {
            cfg!(windows)
                .then(|| env::var_os("USERPROFILE"))
                .flatten()
                .filter(|home| !home.is_empty())
        })
}

fn selected_ghostty_config_path_from_home(home: Option<OsString>) -> Option<PathBuf> {
    let home = PathBuf::from(home?);
    let candidates = ghostty_config_candidate_paths_from_home(&home);
    candidates
        .iter()
        .find(|candidate| candidate.exists())
        .cloned()
        .or_else(|| {
            if cfg!(target_os = "macos") {
                Some(home.join(Path::new(GHOSTTY_CONFIG_DEFAULT_RELATIVE_PATH)))
            } else {
                Some(ghostty_xdg_config_directory(&home).join("config"))
            }
        })
}

fn ghostty_config_candidate_paths_from_home(home: &Path) -> Vec<PathBuf> {
    let mut candidates: Vec<_> = GHOSTTY_CONFIG_CANDIDATE_RELATIVE_PATHS
        .iter()
        .filter(|_| cfg!(target_os = "macos"))
        .map(|relative_path| home.join(Path::new(relative_path)))
        .collect();
    let xdg = ghostty_xdg_config_directory(home);
    candidates.extend([xdg.join("config.ghostty"), xdg.join("config")]);
    candidates
}

fn ghostty_xdg_config_directory(home: &Path) -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".config"))
        .join("ghostty")
}

fn merge_ghostty_config_lines(config: &str, managed_lines: &[&str]) -> String {
    merge_ghostty_managed_config_block(
        config,
        managed_lines
            .iter()
            .map(|line| (*line).to_string())
            .collect(),
    )
}

fn merge_ghostty_terminal_settings(
    config: &str,
    values: &SharedGhosttyTerminalConfigValues,
    changed_keys: &[&str],
) -> String {
    let mut replacing_keys = changed_keys.to_vec();
    if changed_keys.contains(&"theme") {
        for key in GHOSTTY_THEME_MANAGED_COLOR_KEYS {
            if !replacing_keys.contains(key) {
                replacing_keys.push(key);
            }
        }
    }
    merge_ghostty_managed_config_block_entries(
        config,
        &replacing_keys,
        values.managed_config_lines_for_keys(changed_keys),
    )
}

fn merge_ghostty_managed_config_block(config: &str, managed_lines: Vec<String>) -> String {
    let mut retained_lines = Vec::new();
    let mut inside_ghostex_block = false;
    for line in config.lines() {
        let marker = line.trim();
        if marker == GHOSTEX_GHOSTTY_CONFIG_BLOCK_START {
            inside_ghostex_block = true;
            continue;
        }
        if inside_ghostex_block {
            if marker == GHOSTEX_GHOSTTY_CONFIG_BLOCK_END {
                inside_ghostex_block = false;
            }
            continue;
        }
        retained_lines.push(line.to_string());
    }
    let mut next_lines = retained_lines;
    trim_trailing_blank_ghostty_config_lines(&mut next_lines);
    if managed_lines.is_empty() {
        if next_lines.is_empty() {
            return String::new();
        }
        return format!("{}\n", next_lines.join("\n"));
    }
    if next_lines
        .last()
        .is_some_and(|line| !line.trim().is_empty())
    {
        next_lines.push(String::new());
    }
    next_lines.push(GHOSTEX_GHOSTTY_CONFIG_BLOCK_START.to_string());
    next_lines.extend(managed_lines);
    next_lines.push(GHOSTEX_GHOSTTY_CONFIG_BLOCK_END.to_string());
    if next_lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", next_lines.join("\n"))
    }
}

pub(super) fn merge_ghostty_managed_config_block_entries(
    config: &str,
    replacing_keys: &[&str],
    managed_lines: Vec<String>,
) -> String {
    let mut retained_lines = Vec::new();
    let mut retained_block_lines = Vec::new();
    let mut inside_ghostex_block = false;
    for line in config.lines() {
        let marker = line.trim();
        if marker == GHOSTEX_GHOSTTY_CONFIG_BLOCK_START {
            inside_ghostex_block = true;
            continue;
        }
        if inside_ghostex_block {
            if marker == GHOSTEX_GHOSTTY_CONFIG_BLOCK_END {
                inside_ghostex_block = false;
            } else if !replacing_keys.contains(&read_ghostty_config_key(line).as_str()) {
                retained_block_lines.push(line.to_string());
            }
            continue;
        }
        retained_lines.push(line.to_string());
    }
    trim_trailing_blank_ghostty_config_lines(&mut retained_lines);
    let mut next_block_lines = retained_block_lines;
    next_block_lines.extend(managed_lines);
    trim_trailing_blank_ghostty_config_lines(&mut next_block_lines);
    if next_block_lines.is_empty() {
        if retained_lines.is_empty() {
            return String::new();
        }
        return format!("{}\n", retained_lines.join("\n"));
    }

    let mut next_lines = retained_lines;
    if next_lines
        .last()
        .is_some_and(|line| !line.trim().is_empty())
    {
        next_lines.push(String::new());
    }
    next_lines.push(GHOSTEX_GHOSTTY_CONFIG_BLOCK_START.to_string());
    next_lines.extend(next_block_lines);
    next_lines.push(GHOSTEX_GHOSTTY_CONFIG_BLOCK_END.to_string());
    format!("{}\n", next_lines.join("\n"))
}

fn trim_trailing_blank_ghostty_config_lines(lines: &mut Vec<String>) {
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
}

pub(super) fn read_ghostty_config_key(line: &str) -> String {
    let trimmed_line = line.trim();
    if trimmed_line.is_empty() || trimmed_line.starts_with('#') {
        return String::new();
    }
    trimmed_line
        .split_once('=')
        .map(|(key, _)| key.trim().to_string())
        .unwrap_or_else(|| trimmed_line.trim().to_string())
}

pub(super) fn atomic_write_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    ghostex_paths::durable_file::write_durably(path, bytes)
}
