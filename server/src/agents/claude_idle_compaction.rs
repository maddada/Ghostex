//! Claude Code's idle compaction and prompt cache length, set from Settings > Agents > Claude Code
//! and handed to each Claude launch and resume through its one `--settings` JSON.
//!
//! CDXC:AgentProviders 2026-11-17 DECISION:
//! Maintainer (PR review): Ghostex must not override the user's own Claude settings unless the user changed the setting in Ghostex. So the switch, its size and the one-hour cache reach Claude only when that key is saved in Ghostex's settings file, and then also when the saved value equals Claude's default. Presence is the test, not "differs from the default": comparing to the default cannot tell an untouched switch from one deliberately turned on, which is the bug it replaces (after a `"idleCompaction": false` in a user's own ~/.claude/settings.json kept idle compaction off while Ghostex's switch read on). A saved key goes in the `--settings` flag, which outranks the user, project and local settings files; a key never touched in Ghostex leaves the account's own files in charge. The settings file holds only the keys a save wrote (a control saves its one key, `updateSettingsPatch` on the phone and the CLI likewise), so presence means the user acted; Reset all writes every key and so counts as acting. Supersedes the 2026-11-14 rule that a value went only when it differed from Claude's default.
//! CDXC:AgentProviders 2026-11-14 DECISION:
//! User: "we need to work on automatically compacting after the cache expires. thing is claude actually offers this". Claude Code 2.1.299 and later compact a conversation of 200K tokens or more that sits idle for about 54 minutes, before its one-hour prompt cache expires (`idleCompaction`, on by default; `CLAUDE_CODE_IDLE_COMPACT_MIN_TOKENS` for the size, 100K at the least), and only on a one-hour cache, which a subscription gets and an API key, Bedrock, Vertex or Foundry account does not unless `promptCacheTtl` is "1h". So Ghostex sets Claude's own feature rather than sending `/compact` itself: Settings > Agents > Claude Code has "Compact idle chats before the prompt cache expires" (on), "Only above" 100K to 500K (200K, Claude's default; Q3103: "i want settings to not show if the setting that is enabling them is disabled", so it hides while the switch is off) and "Use a 1-hour prompt cache" (off; Q3102). A value is passed only when it differs from Claude's default, so with the defaults the account's own settings.json decides.
//! WHY: the size goes as `env` in the `--settings` JSON, not into the session's environment: Claude copies that `env` into its process environment at start, reads `CLAUDE_CODE_IDLE_COMPACT_MIN_TOKENS` from it each time idle compaction arms, and lists it among the variables settings may set (checked on Claude Code 2.1.325 with `claude -p --settings '{"env":{…}}'`, which printed the value from a shell command). Claude keeps only the last `--settings`, so these keys join the one the Connectors, Bypass and home-folder steps write (`with_claude_settings`).
//! SEE-ALSO: packages/settings-catalog/src/data/defaults.rs (the three keys), apps/desktop/src/app/window/settings_modal/tabs/agents/claude_context.rs (the rows), server/src/session_chat_options/selection_sources.rs (`idleCompactAt` for the context panel's Cache card), server/src/agents/launch_plan.rs and resume_plan.rs (where it is applied).

use serde_json::{json, Map, Value};

use ghostex_settings_catalog::J;

/// Whether Claude compacts an idle chat before its prompt cache expires (Claude's `idleCompaction`).
pub(crate) const IDLE_COMPACTION_SETTING: &str = "claudeIdleCompaction";
/// The smallest conversation it compacts, in tokens (`CLAUDE_CODE_IDLE_COMPACT_MIN_TOKENS`).
pub(crate) const IDLE_COMPACT_MIN_TOKENS_SETTING: &str = "claudeIdleCompactAbove";
/// Whether every Claude session asks for the one-hour prompt cache (`promptCacheTtl: "1h"`).
pub(crate) const ONE_HOUR_CACHE_SETTING: &str = "claudeOneHourPromptCache";

/// Claude Code's own size floor when nothing sets one.
const CLAUDE_DEFAULT_MIN_TOKENS: u64 = 200_000;
/// The variable Claude reads for that size.
const MIN_TOKENS_VARIABLE: &str = "CLAUDE_CODE_IDLE_COMPACT_MIN_TOKENS";

/// The three settings, read from this computer's settings file with the catalog's defaults, and
/// which of them the file actually holds (`*_saved`): only those reach Claude.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ClaudeCompaction {
    pub(crate) idle_compaction: bool,
    pub(crate) min_tokens: u64,
    pub(crate) one_hour_cache: bool,
    idle_compaction_saved: bool,
    min_tokens_saved: bool,
    one_hour_cache_saved: bool,
}

impl ClaudeCompaction {
    /// From a settings file's value (`None`: no file), each key falling back to its default.
    pub(crate) fn from_settings(settings: Option<&Value>) -> Self {
        let saved = |key: &str| settings.and_then(|settings| settings.get(key));
        let default = |key: &str| ghostex_settings_catalog::default_value(key);
        let saved_flag = |key: &str| saved(key).and_then(Value::as_bool);
        let flag = |key: &str| {
            saved_flag(key)
                .or_else(|| match default(key) {
                    Some(J::Bool(value)) => Some(*value),
                    _ => None,
                })
                .unwrap_or(false)
        };
        let saved_min_tokens =
            saved(IDLE_COMPACT_MIN_TOKENS_SETTING).and_then(|value| match value {
                Value::String(text) => text.trim().parse::<u64>().ok(),
                other => other.as_u64(),
            });
        let min_tokens = saved_min_tokens
            .or_else(|| match default(IDLE_COMPACT_MIN_TOKENS_SETTING) {
                Some(J::Str(text)) => text.parse().ok(),
                _ => None,
            })
            .unwrap_or(CLAUDE_DEFAULT_MIN_TOKENS);
        Self {
            idle_compaction: flag(IDLE_COMPACTION_SETTING),
            min_tokens,
            one_hour_cache: flag(ONE_HOUR_CACHE_SETTING),
            idle_compaction_saved: saved_flag(IDLE_COMPACTION_SETTING).is_some(),
            min_tokens_saved: saved_min_tokens.is_some(),
            one_hour_cache_saved: saved_flag(ONE_HOUR_CACHE_SETTING).is_some(),
        }
    }

    /// This computer's settings.
    pub(crate) fn read() -> Self {
        Self::from_settings(
            crate::session_lifecycle::read_sidebar_settings(&crate::paths::get_gxserver_paths(
                None,
            ))
            .as_ref(),
        )
    }

    /// Writes into Claude's settings JSON only the values saved in Ghostex's settings file, whatever
    /// the account's own settings files say; a value never saved is left to them.
    fn write_into(&self, settings: &mut Map<String, Value>) {
        if self.idle_compaction_saved {
            settings.insert("idleCompaction".to_string(), json!(self.idle_compaction));
        }
        // A size is only worth passing while idle compaction is on (the default is on).
        if self.min_tokens_saved && self.idle_compaction {
            if let Some(env) = settings
                .entry("env")
                .or_insert_with(|| json!({}))
                .as_object_mut()
            {
                env.insert(
                    MIN_TOKENS_VARIABLE.to_string(),
                    json!(self.min_tokens.to_string()),
                );
            }
        }
        if self.one_hour_cache_saved && self.one_hour_cache {
            settings.insert("promptCacheTtl".to_string(), json!("1h"));
        }
    }

    /// Whether anything is saved, so a launch needs the flag at all.
    fn writes_anything(&self) -> bool {
        self.idle_compaction_saved
            || (self.min_tokens_saved && self.idle_compaction)
            || (self.one_hour_cache_saved && self.one_hour_cache)
    }
}

/// `/api/readClaudeCompaction`: the three values as Settings shows them, for the phone's Settings ›
/// Chat, which changes them on a computer through `updateSettingsPatch` like any settings write
/// (apps/mobile/app/src/screens/settings/ClaudeCompactionRows.tsx).
pub(crate) fn read_endpoint_value() -> Value {
    let compaction = ClaudeCompaction::read();
    json!({
        "idleCompaction": compaction.idle_compaction,
        "idleCompactAbove": compaction.min_tokens.to_string(),
        "oneHourPromptCache": compaction.one_hour_cache,
        "compactWorkersBeforeReturn": crate::carry_on_compaction::compact_workers_enabled(),
    })
}

/// `command` with this computer's idle compaction and cache length in its `--settings` JSON, for a
/// Claude launch or resume; any other agent's command is left as it is.
pub(crate) fn with_claude_compaction_settings(family: Option<&str>, command: &str) -> String {
    if cfg!(test) {
        return command.to_string();
    }
    with_compaction(family, command, ClaudeCompaction::read())
}

fn with_compaction(family: Option<&str>, command: &str, compaction: ClaudeCompaction) -> String {
    if family != Some("claude") || command.trim().is_empty() || !compaction.writes_anything() {
        return command.to_string();
    }
    super::home_secret_folders::with_claude_settings(command, |settings| {
        compaction.write_into(settings);
        Some(())
    })
    .unwrap_or_else(|| {
        eprintln!(
            "Ghostex could not hand Claude its idle compaction settings: the command already names its own --settings file."
        );
        command.to_string()
    })
}
