//! The Bots sidebar's projects: one ordinary gxserver project per Hermes profile, flagged in
//! `launchSettings` (`isBot`, `botProfile`) so every client can tell it from the user's projects.
//! The endpoint and the startup pass that run it are `server/bot_sync.rs`.
//!
//! CDXC:Bots 2026-09-26 DECISION:
//! User: while Bots is on, gxserver adds a missing bot project for `HERMES_HOME` (shown as Hermes) and for each `HERMES_HOME/profiles/*`, and never deletes one: a profile that vanished leaves an ordinary project the user can close.
//!
//! CDXC:Bots 2026-09-28 DECISION:
//! User: "+" on a bot is the one way to start a session with it: the Hermes agent, run as `hermes -p <profile>` in the profile's folder, with no agent, model or terminal picker. The default profile's bot runs `hermes -p default`, because plain `hermes` follows the sticky profile `hermes profile use` sets and would open another bot. Supersedes the 2026-09-26 rule that ran plain `hermes` for the default profile.
//!
//! CDXC:Bots 2026-09-26 WHY:
//! gxserver swaps the bot's command in for a Hermes launch into a bot project (`resolve_project_agent_config` calls [`bot_agent_config`]), so every client, a remote machine's included, launches a bot through the ordinary agent launch.
//! It is not a seeded project custom agent: the sidebar HUD takes the global agent roster from the first project that has custom agents, and a newly created bot project was that project, which replaced the user's roster.
//!
//! CDXC:Bots 2026-09-26 DECISION:
//! User: a folder already registered as an ordinary project stays that project, and the bots stay in their own mode.
//! So when `HERMES_HOME` (or a profile folder) is already a project, its bot is skipped rather than taking the project over.
//!
//! SEE-ALSO: server/src/presentation/session_projection.rs (publishes `botProfile` and `botGatewayRunning` on the project row), packages/gx-core/src/sidebar_view/projects.rs (`build_project_meta` reads the profile), apps/desktop/src/app/native_sidebar/project_header.rs (draws the tile and the gateway dot).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde_json::{json, Map, Value};

use crate::agents::helpers::object_from_value;
use crate::domain::{
    find_project_by_path_in, normalize_project_root_path, DomainRepository, DomainResult,
};
use crate::paths::GxserverPaths;

/// The `botProfile` of the bot that stands for `HERMES_HOME` itself.
const DEFAULT_BOT_PROFILE: &str = "default";

/// One Hermes profile a bot project stands for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BotProfile {
    pub(crate) profile: String,
    pub(crate) name: String,
    pub(crate) path: PathBuf,
}

/// `botsHidden` is an inverted Official-extension key that defaults to hidden, so only an explicit
/// `false` in the desktop's settings file turns Bots on.
pub(crate) fn bots_enabled(paths: &GxserverPaths) -> bool {
    crate::session_lifecycle::read_sidebar_settings(paths)
        .and_then(|settings| settings.get("botsHidden").and_then(Value::as_bool))
        == Some(false)
}

/// The name is typed into `hermes -p <profile>`, so only a plain word is a bot: letters, digits,
/// `-` and `_`, as Hermes names its profile folders.
pub(crate) fn is_bot_profile_name(profile: &str) -> bool {
    !profile.is_empty()
        && profile
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

/// A bot's Hermes home: `HERMES_HOME` itself for the default profile, `profiles/<profile>` for the
/// others.
pub(crate) fn bot_profile_home(hermes_home: &Path, profile: &str) -> PathBuf {
    if profile == DEFAULT_BOT_PROFILE {
        hermes_home.to_path_buf()
    } else {
        hermes_home.join("profiles").join(profile)
    }
}

/// The command a bot's "+" runs.
fn bot_launch_command(profile: &str) -> String {
    format!("hermes -p {profile}")
}

/// Whether a project is in the Bots space (a Hermes bot or a GBot): its sessions are chats with a
/// bot runtime, not agent sessions doing a task.
///
/// CDXC:Bots 2026-11-08 DECISION:
/// User, after Autopilot typed "Ghostex: [Autopilot] your last turn stopped before your task was done…" into a GBots bot's greeting ("…I'll follow up in a moment.") and it became the conversation's title: "bug pls fix". No automatic Ghostex message (Autopilot's rules, the self-heal "carry on", a coordinator's stall or idle nudge) goes to a bot conversation; the bot answers people, it has no task to carry on.
pub(crate) fn is_bot_project(project: &Value) -> bool {
    project.pointer("/launchSettings/isBot") == Some(&Value::Bool(true))
}

/// BLOCKING (SQLite). [`is_bot_project`] for a session's project id.
pub(crate) fn is_bot_conversation(repository: &DomainRepository<'_>, project_id: &str) -> bool {
    repository
        .get_project(project_id)
        .ok()
        .flatten()
        .is_some_and(|project| is_bot_project(&project))
}

/// The Hermes profile a bot project stands for. `None` for any other project, and for a stored
/// profile name that is not a plain word.
pub(crate) fn bot_profile(project: &Value) -> Option<&str> {
    let launch_settings = project.get("launchSettings")?;
    if launch_settings.get("isBot") != Some(&Value::Bool(true)) {
        return None;
    }
    launch_settings
        .get("botProfile")?
        .as_str()
        .filter(|profile| is_bot_profile_name(profile))
}

/// The agent config a Hermes launch into a bot project uses: the bot's own profile, named after the
/// bot so a session gxserver titles itself is "Harry Session" as the desktop titles it. `None` for
/// any other project or agent.
pub(crate) fn bot_agent_config(project: &Value, agent_id: &str) -> Option<Map<String, Value>> {
    if !agent_id.trim().eq_ignore_ascii_case("hermes-agent") {
        return None;
    }
    let profile = bot_profile(project)?;
    let mut config = object_from_value(json!({
        "agentId": "hermes-agent",
        "command": bot_launch_command(profile),
        "icon": "hermes-agent",
    }));
    if let Some(name) = project
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        config.insert("name".to_string(), Value::String(name.to_string()));
    }
    Some(config)
}

/// The default profile, then every profile folder in name order. Nothing when Hermes has no home.
pub(crate) fn discover_bot_profiles(hermes_home: &Path) -> Vec<BotProfile> {
    if !hermes_home.is_dir() {
        return Vec::new();
    }
    let mut profiles = vec![BotProfile {
        profile: DEFAULT_BOT_PROFILE.to_string(),
        name: "Hermes".to_string(),
        path: hermes_home.to_path_buf(),
    }];
    let mut named: Vec<BotProfile> = fs::read_dir(hermes_home.join("profiles"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            let profile = entry.file_name().to_str()?.to_string();
            (is_bot_profile_name(&profile) && profile != DEFAULT_BOT_PROFILE).then(|| BotProfile {
                name: display_name(&profile),
                path: entry.path(),
                profile,
            })
        })
        .collect();
    named.sort_by(|left, right| left.profile.cmp(&right.profile));
    profiles.extend(named);
    profiles
}

/// Whether the bot's Hermes gateway runs, read from `gateway_state.json` with no CLI spawn: the
/// profile's own gateway is live, or, for a named profile, the default gateway is live and lists
/// it in `served_profiles`.
///
/// CDXC:Bots 2026-09-26 WHY:
/// A multiplexed Hermes runs one gateway in `HERMES_HOME` for every profile and leaves each profile's own `gateway_state.json` behind saying `running` for a pid that died, so reading only the profile's file drew every served bot as stopped.
/// `served_profiles` is what `hermes profile list` reads for the same column.
pub(crate) fn bot_gateway_running(hermes_home: &Path, profile: &str) -> bool {
    if profile == DEFAULT_BOT_PROFILE {
        return live_gateway_state(hermes_home).is_some();
    }
    live_gateway_state(&bot_profile_home(hermes_home, profile)).is_some()
        || live_gateway_state(hermes_home).is_some_and(|state| {
            state
                .get("served_profiles")
                .and_then(Value::as_array)
                .is_some_and(|served| served.iter().any(|name| name.as_str() == Some(profile)))
        })
}

/// A fact about each bot, published on its project row. The projection only reads it, so building
/// a snapshot stays in memory; the 60s pass in `server/bot_sync.rs` refreshes it.
pub(crate) struct PublishedBotFacts<T>(Mutex<BTreeMap<String, T>>);

impl<T: Copy + PartialEq> PublishedBotFacts<T> {
    pub(crate) const fn new() -> Self {
        Self(Mutex::new(BTreeMap::new()))
    }

    fn published(&self) -> MutexGuard<'_, BTreeMap<String, T>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The fact as last published. A profile nobody has read yet is read now, once.
    pub(crate) fn get(&self, profile: &str, read: impl FnOnce() -> T) -> T {
        let cached = self.published().get(profile).copied();
        cached.unwrap_or_else(|| {
            let value = read();
            self.published().insert(profile.to_string(), value);
            value
        })
    }

    /// Stores freshly read facts and returns the profiles whose fact changed since it was last
    /// published. A profile read for the first time is recorded, not reported.
    pub(crate) fn refresh<'a>(&self, fresh: impl IntoIterator<Item = (&'a str, T)>) -> Vec<String> {
        // Read every profile before the lock is taken, so the projection never waits on a file.
        let fresh: Vec<(&str, T)> = fresh.into_iter().collect();
        let mut published = self.published();
        fresh
            .into_iter()
            .filter(|&(profile, value)| {
                published
                    .insert(profile.to_string(), value)
                    .is_some_and(|before| before != value)
            })
            .map(|(profile, _)| profile.to_string())
            .collect()
    }
}

static PUBLISHED_GATEWAYS: PublishedBotFacts<bool> = PublishedBotFacts::new();

/// Whether a bot's gateway runs, as last read.
pub(crate) fn published_bot_gateway_running(profile: &str) -> bool {
    PUBLISHED_GATEWAYS.get(profile, || {
        bot_gateway_running(&crate::session_chat_hermes::hermes_home(), profile)
    })
}

/// Re-reads each profile's gateway and returns the profiles whose state changed since it was
/// last published.
pub(crate) fn refresh_published_bot_gateways<'a>(
    hermes_home: &Path,
    profiles: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    PUBLISHED_GATEWAYS.refresh(
        profiles
            .into_iter()
            .map(|profile| (profile, bot_gateway_running(hermes_home, profile))),
    )
}

/// A home's `gateway_state.json` when it says `running` and its pid is alive.
fn live_gateway_state(home: &Path) -> Option<Value> {
    let state: Value =
        serde_json::from_str(&fs::read_to_string(home.join("gateway_state.json")).ok()?).ok()?;
    let pid = state.get("pid").and_then(Value::as_u64)?;
    (state.get("gateway_state").and_then(Value::as_str) == Some("running")
        && u32::try_from(pid).is_ok_and(crate::runtime::is_process_running))
    .then_some(state)
}

fn display_name(profile: &str) -> String {
    let mut characters = profile.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => String::new(),
    }
}

/// Adds a project for every profile whose folder no project holds yet, and returns the added rows.
pub(crate) fn sync_bot_projects(
    repository: &DomainRepository<'_>,
    profiles: &[BotProfile],
) -> DomainResult<Vec<Value>> {
    let projects = repository.list_projects()?;
    let mut added = Vec::new();
    for bot in profiles {
        let path = Value::String(bot.path.to_string_lossy().to_string());
        let Ok(normalized) = normalize_project_root_path(Some(&path), "path", false) else {
            continue;
        };
        if find_project_by_path_in(&projects, &normalized).is_some() {
            continue;
        }
        let params = object_from_value(json!({
            "path": normalized,
            "name": bot.name,
            "launchSettings": { "isBot": true, "botProfile": bot.profile },
            // None of the Dev, Build, Test and Setup placeholder Actions a repo gets.
            "deletedDefaultCommandIds": crate::sidebar_hud::default_sidebar_command_ids()
                .collect::<Vec<_>>(),
        }));
        added.push(repository.add_project_path(&params)?);
    }
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::get_gxserver_paths;
    use crate::storage::{initialize_gxserver_storage, open_gxserver_database};

    fn hermes_fixture() -> tempfile::TempDir {
        let home = tempfile::tempdir().expect("tempdir");
        for profile in ["harry", "dobby", "not a word"] {
            fs::create_dir_all(home.path().join("profiles").join(profile)).expect("profile");
        }
        fs::write(home.path().join("profiles").join("notes.md"), "").expect("file");
        home
    }

    #[test]
    fn every_profile_folder_is_a_bot_and_the_home_is_hermes() {
        let home = hermes_fixture();
        let profiles = discover_bot_profiles(home.path());
        let named: Vec<(&str, &str)> = profiles
            .iter()
            .map(|bot| (bot.profile.as_str(), bot.name.as_str()))
            .collect();
        assert_eq!(
            named,
            vec![
                ("default", "Hermes"),
                ("dobby", "Dobby"),
                ("harry", "Harry")
            ]
        );
        assert!(discover_bot_profiles(&home.path().join("missing")).is_empty());
    }

    #[test]
    fn sync_adds_flagged_bots_once_and_leaves_a_users_project_alone() {
        let home = hermes_fixture();
        let storage = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(storage.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        let repository = DomainRepository::new(&db, "S1");
        let mut own = Map::new();
        own.insert(
            "path".to_string(),
            Value::String(home.path().to_string_lossy().to_string()),
        );
        repository.add_project_path(&own).expect("user project");

        let profiles = discover_bot_profiles(home.path());
        let added = sync_bot_projects(&repository, &profiles).expect("sync");
        // No stored Actions: Edit SOUL and Edit config are the bot row's own buttons (gx-core).
        let flags: Vec<(String, Value, Value, Value)> = added
            .iter()
            .map(|project| {
                (
                    project["name"].as_str().unwrap_or_default().to_string(),
                    project["launchSettings"].clone(),
                    project["customAgents"].clone(),
                    project["customCommands"].clone(),
                )
            })
            .collect();
        assert_eq!(
            flags,
            vec![
                (
                    "Dobby".to_string(),
                    json!({ "isBot": true, "botProfile": "dobby" }),
                    json!([]),
                    json!([]),
                ),
                (
                    "Harry".to_string(),
                    json!({ "isBot": true, "botProfile": "harry" }),
                    json!([]),
                    json!([]),
                ),
            ]
        );
        let home_project = repository
            .list_projects()
            .expect("projects")
            .into_iter()
            .find(|project| project["name"] != "Dobby" && project["name"] != "Harry")
            .expect("the user's project");
        assert_eq!(home_project["launchSettings"], json!({}));

        assert!(sync_bot_projects(&repository, &profiles)
            .expect("second sync")
            .is_empty());
    }

    #[test]
    fn a_hermes_launch_into_a_bot_runs_its_profile() {
        let bot =
            |profile: &str| json!({ "launchSettings": { "isBot": true, "botProfile": profile } });
        let command = |project: &Value, agent_id: &str| {
            bot_agent_config(project, agent_id).map(|config| config["command"].clone())
        };
        assert_eq!(
            command(&bot("harry"), "hermes-agent"),
            Some(json!("hermes -p harry"))
        );
        assert_eq!(
            command(&bot(DEFAULT_BOT_PROFILE), "hermes-agent"),
            Some(json!("hermes -p default"))
        );
        assert_eq!(command(&bot("harry"), "claude"), None);
        assert_eq!(command(&bot("harry; rm -rf ~"), "hermes-agent"), None);
        assert_eq!(
            command(&json!({ "launchSettings": {} }), "hermes-agent"),
            None
        );
    }

    #[test]
    fn a_bots_gateway_runs_when_its_state_file_or_the_multiplexer_says_so() {
        let home = tempfile::tempdir().expect("tempdir");
        let live = std::process::id();
        let dead = {
            let mut child = std::process::Command::new("true").spawn().expect("spawn");
            let pid = child.id();
            child.wait().expect("wait");
            pid
        };
        let write = |dir: &Path, state: Value| {
            fs::create_dir_all(dir).expect("dir");
            fs::write(dir.join("gateway_state.json"), state.to_string()).expect("state");
        };
        let profile = |name: &str| home.path().join("profiles").join(name);
        write(
            &profile("harry"),
            json!({ "pid": live, "gateway_state": "running" }),
        );
        write(
            &profile("dobby"),
            json!({ "pid": live, "gateway_state": "stopped" }),
        );
        write(
            &profile("content"),
            json!({ "pid": dead, "gateway_state": "running" }),
        );
        fs::create_dir_all(profile("researcher")).expect("dir");
        let running = |name: &str| bot_gateway_running(home.path(), name);

        assert!(running("harry"));
        assert!(!running("dobby"));
        assert!(!running("content"), "a dead pid is a stopped gateway");
        assert!(!running("researcher"), "no state file is a stopped gateway");
        assert!(!running(DEFAULT_BOT_PROFILE));

        write(
            home.path(),
            json!({ "pid": live, "gateway_state": "running", "served_profiles": ["default", "content"] }),
        );
        assert!(running(DEFAULT_BOT_PROFILE));
        assert!(running("content"), "the default gateway serves it");
        assert!(
            !running("researcher"),
            "the default gateway does not serve it"
        );
        assert!(!running("dobby"));

        write(
            home.path(),
            json!({ "pid": dead, "gateway_state": "running", "served_profiles": ["content"] }),
        );
        assert!(!running(DEFAULT_BOT_PROFILE));
        assert!(!running("content"), "a dead multiplexer serves nobody");
    }

    #[test]
    fn bots_are_on_only_when_the_settings_say_false() {
        let storage = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(storage.path().to_path_buf()));
        fs::create_dir_all(&paths.app_config_dir).expect("config dir");
        let settings = paths.app_config_dir.join("native-sidebar-settings.json");
        assert!(!bots_enabled(&paths));
        fs::write(&settings, r#"{"botsHidden":true}"#).expect("settings");
        assert!(!bots_enabled(&paths));
        fs::write(&settings, r#"{"botsHidden":false}"#).expect("settings");
        assert!(bots_enabled(&paths));
    }
}
