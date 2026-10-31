//! Which modes a session's agent cannot use, for its mode menu: Claude's Bypass from the start when
//! the session was launched without the flag that allows it, and for any agent a mode it refused.

use std::collections::{BTreeMap, HashSet};
use std::sync::{Mutex, OnceLock};

use ghostex_gx_protocol::UnavailableMode;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::DomainRepository;
use crate::server::AppState;
use crate::session_chat_compacting::SessionChatCompactingPublisher;
use crate::storage::open_gxserver_database;

/// Runtime-settings key holding a session's refused modes, a JSON string.
pub(crate) const REFUSED_MODES_KEY: &str = "sessionChatRefusedModes";

const REFUSAL_START: &str = "The agent does not offer the ";
const REFUSAL_MIDDLE: &str = " mode in this session; its mode cycle is ";

/// The modes an agent refused, kept with the command it was running: a different command means
/// the session's flags changed, and the refusal says nothing about them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RefusedModes {
    #[serde(default)]
    command: String,
    #[serde(default)]
    modes: BTreeMap<String, String>,
}

/// What a mode the agent's cycle left out answers. `parse_refusal` reads the same words back.
pub(crate) fn refusal_message(target: &str, cycle: &[String]) -> String {
    format!(
        "{REFUSAL_START}{target}{REFUSAL_MIDDLE}{}.",
        cycle.join(" → ")
    )
}

fn parse_refusal(message: &str) -> Option<(String, Vec<String>)> {
    let rest = message.strip_prefix(REFUSAL_START)?;
    let (target, cycle) = rest.split_once(REFUSAL_MIDDLE)?;
    let cycle = cycle
        .trim_end_matches('.')
        .split(" → ")
        .map(str::to_string)
        .collect();
    Some((target.to_string(), cycle))
}

fn launch_command(session: &Value) -> Option<&str> {
    session
        .pointer("/launchSettings/agentLaunchPlan/command")
        .or_else(|| session.pointer("/runtimeSettings/agentCommand"))
        .and_then(Value::as_str)
        .filter(|command| !command.trim().is_empty())
}

/// Whether a command line is Claude Code itself, so that its flags are all there is to read. A
/// wrapper script of the user's own could pass anything.
fn command_runs_claude(command: &str) -> bool {
    let first = command
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_matches(['"', '\'']);
    let program = first.rsplit(['/', '\\']).next().unwrap_or(first);
    matches!(
        program.strip_suffix(".exe").unwrap_or(program),
        "claude" | "cswap"
    )
}

/// Claude Code's Shift+Tab cycle holds Bypass only for a session started in it or allowed to use
/// it (`--allow-dangerously-skip-permissions`).
fn claude_command_offers_bypass(command: &str) -> bool {
    crate::agents::claude_command_starts_in_bypass(command)
        || command
            .split_whitespace()
            .any(|token| token.trim_matches(['"', '\'']) == "--allow-dangerously-skip-permissions")
}

fn mode_label(mode: &str) -> String {
    let label = mode.replace('-', " ");
    let mut chars = label.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => label,
    }
}

const CLAUDE_BYPASS_HOW_TO: &str = "This session wasn't started with the skip-permissions flag. To use Bypass, set Agent approvals to Accept all in Settings, Agents, and start a new session.";

/// How to turn a refused mode on, saying only what is true of that agent. Claude's flag is the
/// one switch known to bring a mode back. Antigravity CLI 1.4 offered Auto with and without its
/// flags here, while the same version refused it on the reporter's Windows machine (GitHub #374),
/// so no switch is named for it; the words say what the agent offers instead of guessing at one.
fn how_to_turn_on(agent: &str, mode: &str, cycle: &[String]) -> String {
    let offered = cycle
        .iter()
        .map(|mode| mode_label(mode))
        .collect::<Vec<_>>()
        .join(", ");
    let not_offered = if offered.is_empty() {
        format!("{} isn't offered in this session.", mode_label(mode))
    } else {
        format!(
            "{} isn't offered in this session, which has {offered}.",
            mode_label(mode)
        )
    };
    match (agent, mode) {
        ("claude", "bypass") => CLAUDE_BYPASS_HOW_TO.to_string(),
        _ => format!("{not_offered} Ghostex checks again when the agent restarts."),
    }
}

fn offered_by_screen() -> &'static Mutex<HashSet<String>> {
    static OFFERED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    OFFERED.get_or_init(Mutex::default)
}

fn session_key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}|{session_id}")
}

/// The footer showed this Claude in Bypass, so Bypass can be used whatever its command says: a
/// `permissions.defaultMode` in Claude's own settings starts it there with no flag.
pub(crate) fn note_screen_mode(project_id: &str, session_id: &str, mode: Option<&str>) {
    if mode == Some("bypass") {
        if let Ok(mut offered) = offered_by_screen().lock() {
            offered.insert(session_key(project_id, session_id));
        }
    }
}

/// CDXC:SessionChat 2026-10-31 DECISION:
/// GitHub #374, user: a mode the session can't use is greyed out, with how to turn it on: from the start for Claude's Bypass when Ghostex knows the session wasn't started with the skip-permissions flag, and after the first refusal for other agents. The refusal is remembered for that session and cleared when the agent restarts or the session's flags change.
/// WHY: Claude Code leaves Bypass out of its Shift+Tab cycle unless it was started with `--dangerously-skip-permissions`, `--permission-mode bypassPermissions` or `--allow-dangerously-skip-permissions`, and this is the same reading of the command the Bypass consent uses (agents/bypass_consent.rs: the auto-accept only rides a launch that starts in bypass, which is never greyed). Only a command that is Claude's own is read: a wrapper script may pass the flag, and a known refusal then still greys it.
/// SEE-ALSO: session_chat_selection_options.rs (`drive_mode_option` finds the refusal), packages/gx-chat-core/src/menus/option_menus.rs, apps/desktop/src/app/native_chat/option_menu/, apps/mobile/app/src/chat/native/composer/menus.ts.
pub(crate) fn unavailable_modes(session: &Value) -> Vec<UnavailableMode> {
    let agent = crate::agents::identity::normalize_agent_id(
        crate::session_chat_follower::session_chat_agent_for_session(session).as_deref(),
    )
    .unwrap_or_default();
    let command = launch_command(session).unwrap_or_default();
    let mut modes = refused_modes(session)
        .filter(|refused| refused.command == command)
        .map(|refused| refused.modes)
        .unwrap_or_default();
    // The stored agent command leaves out what Ghostex adds at launch (Accept all's flag), so only
    // the launch plan says what the session was really started with.
    let launched = session
        .pointer("/launchSettings/agentLaunchPlan/command")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if agent == "claude"
        && command_runs_claude(launched)
        && !claude_command_offers_bypass(launched)
        && !session_showed_bypass(session)
    {
        modes
            .entry("bypass".to_string())
            .or_insert_with(|| CLAUDE_BYPASS_HOW_TO.to_string());
    }
    modes
        .into_iter()
        .map(|(value, why)| UnavailableMode { value, why })
        .collect()
}

fn session_showed_bypass(session: &Value) -> bool {
    let (Some(project_id), Some(session_id)) = (
        session.get("projectId").and_then(Value::as_str),
        session.get("sessionId").and_then(Value::as_str),
    ) else {
        return false;
    };
    offered_by_screen()
        .lock()
        .is_ok_and(|offered| offered.contains(&session_key(project_id, session_id)))
}

fn refused_modes(session: &Value) -> Option<RefusedModes> {
    session
        .pointer(&format!("/runtimeSettings/{REFUSED_MODES_KEY}"))
        .and_then(Value::as_str)
        .and_then(|raw| serde_json::from_str(raw).ok())
}

/// The queue snapshot's reading of it, so it rides every frame and read the queue does. A direct
/// read of the three columns: the scheduler takes a snapshot of every queue it ticks.
pub(crate) fn read_unavailable_modes(
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
) -> Vec<UnavailableMode> {
    let json_column = |row: &rusqlite::Row<'_>, index: usize| -> Value {
        row.get::<_, String>(index)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or(Value::Null)
    };
    let session = db.query_row(
        "SELECT agentId, launchSettingsJson, runtimeSettingsJson FROM sessions WHERE projectId = ?1 AND sessionId = ?2",
        rusqlite::params![project_id, session_id],
        |row| {
            Ok(serde_json::json!({
                "projectId": project_id,
                "sessionId": session_id,
                "agentId": row.get::<_, Option<String>>(0)?,
                "launchSettings": json_column(row, 1),
                "runtimeSettings": json_column(row, 2),
            }))
        },
    );
    session
        .map(|session| unavailable_modes(&session))
        .unwrap_or_default()
}

/// Remembers a mode the agent's own cycle left out, when `message` is that refusal.
pub(crate) fn remember_refusal(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    message: &str,
) {
    let Some((target, cycle)) = parse_refusal(message) else {
        return;
    };
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return;
    };
    let agent = crate::agents::identity::normalize_agent_id(
        crate::session_chat_follower::session_chat_agent_for_session(&session).as_deref(),
    )
    .unwrap_or_default();
    let command = launch_command(&session).unwrap_or_default().to_string();
    let mut refused = refused_modes(&session)
        .filter(|refused| refused.command == command)
        .unwrap_or_default();
    refused.command = command;
    refused
        .modes
        .insert(target.clone(), how_to_turn_on(&agent, &target, &cycle));
    let Ok(raw) = serde_json::to_string(&refused) else {
        return;
    };
    SessionChatCompactingPublisher::new(state).publish_marker(
        project_id,
        session_id,
        REFUSED_MODES_KEY,
        Some(&raw),
    );
}

/// The agent restarted or stopped: what its last run offered says nothing about the next one.
pub(crate) fn forget_for_agent_restart(
    publisher: &SessionChatCompactingPublisher,
    project_id: &str,
    session_id: &str,
) {
    if let Ok(mut offered) = offered_by_screen().lock() {
        offered.remove(&session_key(project_id, session_id));
    }
    publisher.publish_marker(project_id, session_id, REFUSED_MODES_KEY, None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn session(agent: &str, command: &str) -> Value {
        json!({
            "agentId": agent,
            "projectId": "P",
            "sessionId": format!("S{agent}{command}"),
            "launchSettings": { "agentLaunchPlan": { "command": command } },
            "runtimeSettings": { "agentCommand": agent },
        })
    }

    #[test]
    fn claude_without_the_flag_has_bypass_greyed_from_the_start() {
        let modes = unavailable_modes(&session("claude", "claude --model haiku"));
        assert_eq!(modes.len(), 1);
        assert_eq!(modes[0].value, "bypass");
        assert!(modes[0].why.contains("skip-permissions flag"));
        assert!(modes[0].why.contains("Accept all"));
    }

    #[test]
    fn a_claude_that_can_use_bypass_keeps_it() {
        for command in [
            "claude --dangerously-skip-permissions",
            "claude --allow-dangerously-skip-permissions",
            "claude --permission-mode bypassPermissions",
            "cswap run 2 --share-history -- --dangerously-skip-permissions --model opus",
        ] {
            assert_eq!(
                unavailable_modes(&session("claude", command)),
                vec![],
                "{command}"
            );
        }
    }

    #[test]
    fn a_wrapper_script_is_not_read_as_having_no_flag() {
        assert_eq!(
            unavailable_modes(&session("claude", "my-claude-wrapper")),
            vec![]
        );
    }

    #[test]
    fn an_unknown_command_greys_nothing() {
        let session = json!({
            "agentId": "claude",
            "runtimeSettings": { "agentCommand": "claude --model haiku" },
            "launchSettings": {},
        });
        assert_eq!(unavailable_modes(&session), vec![]);
    }

    #[test]
    fn a_screen_that_showed_bypass_lifts_the_grey() {
        let mut claude = session("claude", "claude");
        claude["sessionId"] = json!("Sshown");
        assert_eq!(unavailable_modes(&claude).len(), 1);
        note_screen_mode("P", "Sshown", Some("bypass"));
        assert_eq!(unavailable_modes(&claude), vec![]);
    }

    #[test]
    fn other_agents_are_greyed_only_after_a_refusal() {
        let mut agy = session("antigravity", "agy");
        assert_eq!(unavailable_modes(&agy), vec![]);
        let message = refusal_message(
            "auto",
            &[
                "accept-edits".to_string(),
                "plan".to_string(),
                "default".to_string(),
            ],
        );
        assert_eq!(
            message,
            "The agent does not offer the auto mode in this session; its mode cycle is accept-edits → plan → default."
        );
        let (target, cycle) = parse_refusal(&message).unwrap();
        let mut modes = BTreeMap::new();
        modes.insert(
            target.clone(),
            how_to_turn_on("antigravity", &target, &cycle),
        );
        let raw = serde_json::to_string(&RefusedModes {
            command: "agy".to_string(),
            modes,
        })
        .unwrap();
        agy["runtimeSettings"][REFUSED_MODES_KEY] = json!(raw);
        let modes = unavailable_modes(&agy);
        assert_eq!(modes[0].value, "auto");
        assert_eq!(
            modes[0].why,
            "Auto isn't offered in this session, which has Accept edits, Plan, Default. Ghostex checks again when the agent restarts."
        );
    }

    #[test]
    fn a_refusal_ends_when_the_flags_change() {
        let mut agy = session("antigravity", "agy");
        let raw = serde_json::to_string(&RefusedModes {
            command: "agy --mode plan".to_string(),
            modes: BTreeMap::from([("auto".to_string(), "no".to_string())]),
        })
        .unwrap();
        agy["runtimeSettings"][REFUSED_MODES_KEY] = json!(raw);
        assert_eq!(unavailable_modes(&agy), vec![]);
    }

    #[test]
    fn an_agent_with_no_known_switch_gets_no_invented_one() {
        let why = how_to_turn_on("codex", "plan", &["default".to_string()]);
        assert_eq!(
            why,
            "Plan isn't offered in this session, which has Default. Ghostex checks again when the agent restarts."
        );
    }

    #[test]
    fn claude_refusing_bypass_says_how_to_turn_it_on() {
        let why = how_to_turn_on(
            "claude",
            "bypass",
            &["auto".to_string(), "manual".to_string()],
        );
        assert_eq!(why, CLAUDE_BYPASS_HOW_TO);
    }
}
