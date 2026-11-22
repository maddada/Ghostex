//! Ghostex's skills in a Claude that Ghostex did not start answer only to their slash command.
//!
//! Every Claude on the computer reads `~/.claude/skills`, where Ghostex links its skills. The
//! bundled ones are all manual-only in their own frontmatter (`disable-model-invocation: true`);
//! a message that names one (`/ghostex-coordinator` in a coordinator's first message) still loads
//! it. An extension's skill (`thread-brief`) may come without that flag. For such a skill gxserver
//! writes `"skillOverrides": {"<name>": "user-invocable-only"}` into each Claude user settings file
//! (hidden from the model, `/name` still works), and every Claude command Ghostex starts or resumes
//! carries `"<name>": "on"` in its `--settings` JSON, which outranks user settings. An entry Ghostex
//! wrote for a skill that has since gained the flag itself is removed: the flag does the same.
//!
//! CDXC:AgentSkills 2026-11-17 DECISION:
//! User: sessions launched outside Ghostex "must not get or use the Ghostex skills on their own";
//! they are used only when the user asks with the slash command (`/ghostex-coordinator`,
//! `/ghostex-help`), or when Ghostex launches the session. A plain `claude` on the user's Linux
//! computer loaded the coordinator playbook by itself and ran `ghostex coordinator item ask`.
//! Then (Q1 "C") the four bundled skills that still loaded on their own (coordinator, GBots,
//! secrets, agent guide) got the flag too, so this file now serves extension skills.
//! WHY not a separate folder per session (`--add-dir`): tested on Claude Code 2.1.326, a skill in
//! an added folder whose name is also in `~/.claude/skills` is offered to the model as
//! `<folder>:<name>`, "scoped to <folder>/", and `skillOverrides` "off" hides both copies, so a
//! manual-only global copy and a normal per-session copy can't live side by side. A skill's own
//! `disable-model-invocation` can't be turned back on by settings ("locked_by: author"). A
//! manual-only skill named in the user's message as `/name` is still loaded by the Skill tool
//! (tested with haiku: "Read and adhere to this skill: /gx-probe" loaded it; the same request without
//! `/gx-probe` was refused). A key the user set themselves is left alone, in their file and on the
//! launch.
//! SEE-ALSO: server/src/agents/launch_plan.rs and resume_plan.rs (the `--settings` merge, next to
//! the idle compaction one), server/src/agents/home_secret_folders.rs (`with_claude_settings`).

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::{json, Map, Value};

use crate::agent_skills::{
    path_string, CANONICAL_AGENT_SKILLS_DIR, GHOSTEX_AGENT_SKILL_NAMES,
    RETIRED_GHOSTEX_AGENT_SKILL_NAMES,
};
use crate::paths::GxserverPaths;

const SKILL_OVERRIDES_KEY: &str = "skillOverrides";
/// Hidden from the model; `/name` still works.
const OUTSIDE_GHOSTEX: &str = "user-invocable-only";
const IN_A_GHOSTEX_SESSION: &str = "on";

/// The file an extension's installed skill carries naming the extension (extensions/skills.rs).
const EXTENSION_SKILL_MARKER: &str = ".ghostex-extension";

/// Whether the `~/.agents/skills` entry `name` is one Ghostex put there: a bundled skill (current
/// or retired name, or an old name of a renamed one) or an extension's skill (its marker file).
pub(crate) fn is_ghostex_skill(canonical_root: &Path, name: &str) -> bool {
    GHOSTEX_AGENT_SKILL_NAMES.contains(&name)
        || RETIRED_GHOSTEX_AGENT_SKILL_NAMES.contains(&name)
        || crate::coordinators::COORDINATOR_SKILL_OLD_NAMES.contains(&name)
        || canonical_root
            .join(name)
            .join(EXTENSION_SKILL_MARKER)
            .is_file()
}

/// Whether the SKILL.md frontmatter says `disable-model-invocation: true`.
fn frontmatter_disables_model_invocation(markdown: &str) -> bool {
    let mut lines = markdown.lines();
    if lines.next().map(str::trim) != Some("---") {
        return false;
    }
    lines
        .take_while(|line| line.trim() != "---")
        .filter_map(|line| line.trim().strip_prefix("disable-model-invocation:"))
        .any(|value| value.trim().trim_matches(['"', '\'']) == "true")
}

/// The Ghostex skills installed in `~/.agents/skills`, by name, split into the ones a model may
/// pick up on its own and the ones whose frontmatter makes them manual-only.
fn ghostex_skills_by_invocation(home: &Path) -> (Vec<String>, Vec<String>) {
    let canonical_root = home.join(CANONICAL_AGENT_SKILLS_DIR);
    let mut model_invocable = Vec::new();
    let mut manual_only = Vec::new();
    for name in ghostex_agent_sync::fsx::child_names(&canonical_root) {
        if name.starts_with('.') || !is_ghostex_skill(&canonical_root, &name) {
            continue;
        }
        match fs::read_to_string(canonical_root.join(&name).join("SKILL.md")) {
            Ok(markdown) if frontmatter_disables_model_invocation(&markdown) => {
                manual_only.push(name)
            }
            Ok(_) => model_invocable.push(name),
            Err(_) => {}
        }
    }
    model_invocable.sort();
    manual_only.sort();
    (model_invocable, manual_only)
}

/// The Ghostex skills installed in `~/.agents/skills` that a model may pick up on its own, by name.
pub(crate) fn model_invocable_ghostex_skills(home: &Path) -> Vec<String> {
    ghostex_skills_by_invocation(home).0
}

/// Every Claude user settings file a plain `claude` may read: the default config folder's
/// (created when the folder exists), each cswap account's and each `~/.claude-profiles` profile's
/// that exists, once each (cswap links its accounts' file to the default one).
pub(crate) fn claude_user_settings_files(home: &Path) -> Vec<PathBuf> {
    let default_home = ghostex_paths::agent_homes::claude_default_home(home);
    let mut files = Vec::new();
    if default_home.is_dir() {
        files.push(default_home.join("settings.json"));
    }
    files.extend(
        crate::agent_folder_trust::files::existing_files_in_subfolders(
            &crate::accounts::claude_resets::swap_root(home).join("sessions"),
            "settings.json",
        ),
    );
    files.extend(
        crate::agent_folder_trust::files::existing_files_in_subfolders(
            &home.join(".claude-profiles"),
            "settings.json",
        ),
    );
    crate::agent_folder_trust::files::dedupe_files(files)
}

/// Adds `"<name>": "user-invocable-only"` under `skillOverrides` for each of `names` the file
/// doesn't name yet, and removes that same value for each of `drop` (skills manual-only by their
/// own flag now). Returns whether the file changed. A file that isn't a JSON object is an error
/// and is never rewritten; a file that doesn't exist is created only to add an entry.
fn hide_from_model_in(file: &Path, names: &[String], drop: &[String]) -> std::io::Result<bool> {
    use crate::agent_folder_trust::files::{read_json_object, write_json_object};
    let existing = read_json_object(file)?;
    if existing.is_none() && names.is_empty() {
        return Ok(false);
    }
    let mut document = existing.unwrap_or_default();
    let had_overrides = document.object.contains_key(SKILL_OVERRIDES_KEY);
    let overrides = document
        .object
        .entry(SKILL_OVERRIDES_KEY.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(overrides) = overrides.as_object_mut() else {
        return Err(std::io::Error::other("its skillOverrides is not an object"));
    };
    let mut changed = false;
    for name in names {
        if !overrides.contains_key(name) {
            overrides.insert(name.clone(), json!(OUTSIDE_GHOSTEX));
            changed = true;
        }
    }
    for name in drop {
        if overrides.get(name).and_then(Value::as_str) == Some(OUTSIDE_GHOSTEX) {
            overrides.remove(name);
            changed = true;
        }
    }
    if overrides.is_empty() && (changed || !had_overrides) {
        document.object.remove(SKILL_OVERRIDES_KEY);
    }
    if changed {
        write_json_object(file, &document)?;
    }
    Ok(changed)
}

/// Makes every Claude user settings file hide Ghostex's model-invocable skills from the model, and
/// drops the entries it wrote for skills that are manual-only by their own flag now. Returns the
/// files changed and the ones that could not be.
pub(crate) fn keep_ghostex_skills_manual_outside_ghostex(home: &Path) -> (Vec<String>, Vec<Value>) {
    let (names, manual_only) = ghostex_skills_by_invocation(home);
    let mut changed = Vec::new();
    let mut failed = Vec::new();
    for file in claude_user_settings_files(home) {
        match hide_from_model_in(&file, &names, &manual_only) {
            Ok(true) => changed.push(path_string(&file)),
            Ok(false) => {}
            Err(error) => failed.push(json!({
                "error": error.to_string(),
                "path": path_string(&file),
            })),
        }
    }
    (changed, failed)
}

/// The overrides a Claude command Ghostex starts gets: `"on"` for each model-invocable Ghostex
/// skill, except one the user set themselves in their default settings file (any value other than
/// the one Ghostex writes).
fn session_overrides(home: &Path) -> Map<String, Value> {
    let users_own: Map<String, Value> = crate::agent_folder_trust::files::read_json_object(
        &ghostex_paths::agent_homes::claude_default_home(home).join("settings.json"),
    )
    .ok()
    .flatten()
    .and_then(|file| file.object.get(SKILL_OVERRIDES_KEY).cloned())
    .and_then(|value| value.as_object().cloned())
    .unwrap_or_default();
    model_invocable_ghostex_skills(home)
        .into_iter()
        .filter(|name| {
            users_own
                .get(name)
                .is_none_or(|value| value.as_str() == Some(OUTSIDE_GHOSTEX))
        })
        .map(|name| (name, json!(IN_A_GHOSTEX_SESSION)))
        .collect()
}

/// Write Ghostex's `"skillOverrides"` directly into a Ghostex-generated Claude `--settings` map
/// (used by Non-CLI `driver_claude`).
pub(crate) fn write_ghostex_skills_into_settings(settings: &mut Map<String, Value>) {
    if cfg!(test) {
        return;
    }
    let home = crate::paths::get_gxserver_paths(None)
        .agent_config_home_dir()
        .to_path_buf();
    let _ = keep_ghostex_skills_manual_outside_ghostex(&home);
    let overrides = session_overrides(&home);
    if overrides.is_empty() {
        return;
    }
    if let Some(entry) = settings
        .entry(SKILL_OVERRIDES_KEY.to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
    {
        for (name, value) in overrides {
            entry.entry(name).or_insert(value);
        }
    }
}

/// `command` with `"skillOverrides": {"<name>": "on"}` merged into its `--settings` JSON when it
/// starts or resumes Claude, so a Ghostex session's model may still pick up Ghostex's skills.
pub(crate) fn with_ghostex_skills_on(family: Option<&str>, command: &str) -> String {
    // Unit tests assert exact commands, which must not depend on the skills on the machine.
    if cfg!(test) {
        return command.to_string();
    }
    let home = crate::paths::get_gxserver_paths(None)
        .agent_config_home_dir()
        .to_path_buf();
    with_overrides(family, command, &home)
}

fn with_overrides(family: Option<&str>, command: &str, home: &Path) -> String {
    if family != Some("claude") || command.trim().is_empty() {
        return command.to_string();
    }
    // Keeps plain sessions current too, for a skill installed since gxserver started.
    let _ = keep_ghostex_skills_manual_outside_ghostex(home);
    let overrides = session_overrides(home);
    if overrides.is_empty() {
        return command.to_string();
    }
    crate::agents::with_claude_settings(command, |settings| {
        let entry = settings
            .entry(SKILL_OVERRIDES_KEY.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        let entry = entry.as_object_mut()?;
        for (name, value) in overrides {
            entry.entry(name).or_insert(value);
        }
        Some(())
    })
    .unwrap_or_else(|| {
        eprintln!(
            "Ghostex could not let Claude pick up Ghostex's skills on its own: the command already names its own --settings file."
        );
        command.to_string()
    })
}

/// Startup pass. Returns a report when a file changed or failed, for the log.
pub fn keep_ghostex_skills_manual_at_startup(paths: &GxserverPaths) -> Option<Value> {
    let (changed, failed) =
        keep_ghostex_skills_manual_outside_ghostex(paths.agent_config_home_dir());
    if changed.is_empty() && failed.is_empty() {
        return None;
    }
    Some(json!({
        "type": "ghostexSkillsManualOutsideGhostex",
        "changed": changed,
        "failed": failed,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_skill(dir: &Path, name: &str, manual: bool) {
        fs::create_dir_all(dir).unwrap();
        let flag = if manual {
            "disable-model-invocation: true\n"
        } else {
            ""
        };
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: test\n{flag}---\nBody\n"),
        )
        .unwrap();
    }

    fn home_with_skills() -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        let skills = temp.path().join(".agents/skills");
        write_skill(
            &skills.join("ghostex-coordinator"),
            "ghostex-coordinator",
            false,
        );
        write_skill(&skills.join("ghostex-help"), "ghostex-help", true);
        write_skill(&skills.join("thread-brief"), "thread-brief", false);
        fs::write(
            skills.join("thread-brief").join(EXTENSION_SKILL_MARKER),
            "x",
        )
        .unwrap();
        write_skill(&skills.join("users-own"), "users-own", false);
        fs::create_dir_all(temp.path().join(".claude")).unwrap();
        temp
    }

    #[test]
    fn only_ghostex_skills_the_model_may_pick_up_are_listed() {
        let home = home_with_skills();
        assert_eq!(
            model_invocable_ghostex_skills(home.path()),
            ["ghostex-coordinator", "thread-brief"]
        );
    }

    #[test]
    fn plain_claude_gets_them_slash_only_and_the_users_own_choice_stays() {
        let home = home_with_skills();
        let settings = home.path().join(".claude/settings.json");
        fs::write(
            &settings,
            r#"{"model":"opus","skillOverrides":{"thread-brief":"off"}}"#,
        )
        .unwrap();
        let (changed, failed) = keep_ghostex_skills_manual_outside_ghostex(home.path());
        assert!(failed.is_empty(), "{failed:?}");
        assert_eq!(changed.len(), 1);
        let written: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(written["model"], "opus");
        assert_eq!(
            written["skillOverrides"],
            json!({"ghostex-coordinator": "user-invocable-only", "thread-brief": "off"})
        );
        // A second pass changes nothing.
        assert!(keep_ghostex_skills_manual_outside_ghostex(home.path())
            .0
            .is_empty());
    }

    #[test]
    fn an_entry_ghostex_wrote_goes_once_the_skill_carries_the_flag_itself() {
        let home = home_with_skills();
        let settings = home.path().join(".claude/settings.json");
        fs::write(
            &settings,
            r#"{"skillOverrides":{"ghostex-help":"user-invocable-only","ghostex-cli":"off"}}"#,
        )
        .unwrap();
        write_skill(
            &home.path().join(".agents/skills/ghostex-cli"),
            "ghostex-cli",
            true,
        );
        keep_ghostex_skills_manual_outside_ghostex(home.path());
        let written: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
        // ghostex-help's flag does the same now; the user's own "off" stays.
        assert_eq!(
            written["skillOverrides"],
            json!({
                "ghostex-cli": "off",
                "ghostex-coordinator": "user-invocable-only",
                "thread-brief": "user-invocable-only"
            })
        );
    }

    #[test]
    fn a_settings_file_that_is_not_json_is_left_alone() {
        let home = home_with_skills();
        let settings = home.path().join(".claude/settings.json");
        fs::write(&settings, "not json").unwrap();
        let (changed, failed) = keep_ghostex_skills_manual_outside_ghostex(home.path());
        assert!(changed.is_empty());
        assert_eq!(failed.len(), 1);
        assert_eq!(fs::read_to_string(&settings).unwrap(), "not json");
    }

    #[test]
    fn a_ghostex_claude_launch_turns_them_back_on_in_its_one_settings_json() {
        let home = home_with_skills();
        fs::write(
            home.path().join(".claude/settings.json"),
            r#"{"skillOverrides":{"thread-brief":"off"}}"#,
        )
        .unwrap();
        let command = with_overrides(
            Some("claude"),
            r#"claude --settings '{"env":{"GH_CONFIG_DIR":"/x"}}'"#,
            home.path(),
        );
        assert_eq!(
            command,
            r#"claude --settings '{"env":{"GH_CONFIG_DIR":"/x"},"skillOverrides":{"ghostex-coordinator":"on"}}'"#
        );
        // Another agent, or a command naming its own settings file, is left alone.
        assert_eq!(with_overrides(Some("codex"), "codex", home.path()), "codex");
        let own_file = "claude --settings /tmp/mine.json";
        assert_eq!(
            with_overrides(Some("claude"), own_file, home.path()),
            own_file
        );
    }

    #[test]
    fn frontmatter_flag_is_read_only_inside_the_frontmatter() {
        assert!(frontmatter_disables_model_invocation(
            "---\nname: a\ndisable-model-invocation: true\n---\n"
        ));
        assert!(!frontmatter_disables_model_invocation(
            "---\nname: a\n---\ndisable-model-invocation: true\n"
        ));
        assert!(!frontmatter_disables_model_invocation("name: a\n"));
    }
}
