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
/// Whether pressing Enter on a long, cold chat offers to compact first (Q4396, on by default).
pub(crate) const OFFER_COMPACT_BEFORE_SENDING_SETTING: &str = "offerCompactBeforeSending";

/// Claude Code's own size floor when nothing sets one.
const CLAUDE_DEFAULT_MIN_TOKENS: u64 = 200_000;
/// Claude Code's hard lower bound for `CLAUDE_CODE_IDLE_COMPACT_MIN_TOKENS` (100K).
const CLAUDE_MIN_TOKENS_FLOOR: u64 = 100_000;
/// Sanity cap on how far in the future a cached `expiresAt` can defer Auto Sleep (2 hours).
const MAX_IDLE_COMPACT_WAIT_MS: i64 = 2 * 60 * 60_000;
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

/// Whether "Offer to compact long chats before sending" is on (on by default).
pub(crate) fn offer_compact_before_sending_enabled() -> bool {
    crate::session_lifecycle::read_sidebar_settings(&crate::paths::get_gxserver_paths(None))
        .and_then(|settings| {
            settings
                .get(OFFER_COMPACT_BEFORE_SENDING_SETTING)
                .and_then(Value::as_bool)
        })
        .unwrap_or(true)
}

/// `/api/readClaudeCompaction`: the values as Settings shows them, for the phone's Settings >
/// Chat, which changes them on a computer through `updateSettingsPatch` like any settings write
/// (apps/mobile/app/src/screens/settings/ClaudeCompactionRows.tsx).
pub(crate) fn read_endpoint_value() -> Value {
    let compaction = ClaudeCompaction::read();
    json!({
        "idleCompaction": compaction.idle_compaction,
        "idleCompactAbove": compaction.min_tokens.to_string(),
        "oneHourPromptCache": compaction.one_hour_cache,
        "offerCompactBeforeSending": offer_compact_before_sending_enabled(),
        "compactWorkersBeforeReturn": crate::carry_on_compaction::compact_workers_enabled(),
    })
}

/// CDXC:SessionSleep 2026-11-22 DECISION:
/// User (Q4395, "sleep waits"): Agent Auto Sleep used to sleep background Claude Code sessions
/// after `autoSleepAgentIdleMinutes` (e.g. 10 min), killing the process before Claude's built-in
/// `idleCompaction` timer could run at ~54 minutes (90% of the 1-hour cache). When a Claude
/// session qualifies for built-in idle compaction (idle compaction on, last measured context at or
/// above Compact chats above, warm 1-hour prompt cache that has not yet expired), Auto Sleep
/// defers sleeping it until idle compaction completes (which clears or shrinks measured context)
/// or the 1-hour cache window ends (`expiresAt`, bounded by `MAX_IDLE_COMPACT_WAIT_MS`), so the
/// session never stays awake forever. Sessions that do not qualify (below threshold, already
/// compacted, 5-minute cache, idle compaction turned off, or non-Claude agents) sleep on the
/// normal schedule. An explicit Sleep from the user still goes through.
pub(crate) fn should_wait_for_idle_compaction(
    compaction: ClaudeCompaction,
    used_tokens: Option<u64>,
    prompt_cache: Option<&Value>,
    now_ms: i64,
) -> bool {
    if !compaction.idle_compaction {
        return false;
    }
    let min_tokens = compaction.min_tokens.max(CLAUDE_MIN_TOKENS_FLOOR);
    let Some(tokens) = used_tokens else {
        return false;
    };
    if tokens < min_tokens {
        return false;
    }
    let Some(cache) = prompt_cache.and_then(Value::as_object) else {
        return false;
    };
    if cache.get("warm").and_then(Value::as_bool) != Some(true) {
        return false;
    }
    if cache.get("ttl").and_then(Value::as_str) != Some("1h") {
        return false;
    }
    let Some(expires_at_secs) = cache
        .get("expiresAt")
        .and_then(|value| value.as_i64().or_else(|| value.as_f64().map(|n| n as i64)))
    else {
        return false;
    };
    let expires_at_ms = expires_at_secs.saturating_mul(1_000);
    expires_at_ms > now_ms && expires_at_ms.saturating_sub(now_ms) <= MAX_IDLE_COMPACT_WAIT_MS
}

/// Whether Auto Sleep should wait for Claude Code's built-in idle compaction on `session`.
pub(crate) fn session_waiting_for_idle_compaction(
    paths: &crate::paths::GxserverPaths,
    session: &Value,
    now_ms: i64,
) -> bool {
    let agent = crate::session_chat_composer::session_chat_composer_agent_id(session)
        .or_else(|| crate::session_chat_follower::session_chat_display_agent_for_session(session));
    if crate::session_chat_options::session_chat_option_agent(agent.as_deref())
        != Some(crate::session_chat_options::SessionChatOptionAgent::Claude)
    {
        return false;
    }
    let compaction = ClaudeCompaction::from_settings(
        crate::shared_settings_file::read_shared_settings(paths).as_ref(),
    );
    if !compaction.idle_compaction {
        return false;
    }
    let Some(agent_session_id) = session
        .pointer("/runtimeSettings/agentSessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
    else {
        return false;
    };
    let hook_state_directory = crate::session_chat_options::session_chat_hook_state_directory(paths);
    let Some((selection, _)) = crate::session_chat_options::read_claude_statusline_reading(
        &hook_state_directory,
        agent_session_id,
        false,
    ) else {
        return false;
    };
    let used_tokens = selection.context_usage.and_then(|usage| {
        usage
            .used_tokens
            .or_else(|| Some(u64::from(usage.used_percentage?) * usage.window_size? / 100))
    });
    let prompt_cache = selection
        .claude_status
        .as_ref()
        .and_then(|status| status.get("promptCache"));
    should_wait_for_idle_compaction(compaction, used_tokens, prompt_cache, now_ms)
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
