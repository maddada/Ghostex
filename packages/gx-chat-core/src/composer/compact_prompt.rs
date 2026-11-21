//! Ask-on-Enter compact-before-sending prompt for long chats whose prompt cache has expired.
//!
//! CDXC:SessionChat 2026-11-22 DECISION:
//! User (Q4396, "ask on enter"): when pressing Enter (or clicking Send) on a long, uncompacted
//! chat (context tokens at or above Settings > Agents > Claude Code > Compact chats above, default
//! 200K) whose prompt cache has expired, on any agent that supports Compact & Send, open a card
//! above the chat box offering to compact first to save usage (Option + Enter / Compact, then send)
//! or send as is (Enter again / Send now). Two "Don't ask again" options sit on the card:
//! - "This chat" (`scope: "session"`): stops asking for the rest of this chat.
//! - "Always" (`scope: "always"`): turns off `offerCompactBeforeSending` in Settings, which can be
//!   switched back on under Settings > Agents > Claude Code.
//!
//! Cache expiry rules:
//! - Claude Code: uses `claudeStatus.promptCache` (`warm == false` or `now_ms >= expiresAt * 1000`),
//!   falling back to 1 hour since the newest transcript row when `promptCache` is absent.
//! - Codex CLI and other compact-capable agents: uses 30 minutes since the newest transcript row
//!   (matching OpenAI's documented 30-minute default minimum prompt cache lifetime on GPT-5.6+).

use serde_json::json;

use crate::composer::compact_hint::CODEX_IDLE_CACHE_MS;
use crate::document::CompactBeforeSendOffer;
use crate::effect::Effect;
use crate::state::{ChatContext, ChatState};
use ghostex_gx_protocol::ChatSource;

/// Default "Compact chats above" threshold when neither host settings nor gxserver supplied one.
pub const DEFAULT_COMPACT_CHATS_ABOVE: u64 = 200_000;

/// Lower bound for the threshold, matching Claude Code's own 100K floor.
const MIN_COMPACT_CHATS_ABOVE: u64 = 100_000;

/// Fallback cache lifetime for Claude chats whose statusline has not reported `promptCache`.
const CLAUDE_FALLBACK_CACHE_MS: i64 = 60 * 60 * 1000;

/// Default idle cache lifetime for other compact-capable agents (30 minutes).
const OTHER_AGENT_IDLE_CACHE_MS: i64 = CODEX_IDLE_CACHE_MS;

/// The timer that redraws an open chat when its prompt cache expires.
pub const COMPACT_PROMPT_TIMER: &str = "composer.compactPrompt";

/// Host action emitted when the user picks "Always" under "Don't ask again".
pub const DISABLE_OFFER_HOST_ACTION: &str = "disableOfferCompactBeforeSending";

enum CacheStatus {
    /// Prompt cache is currently warm; `expires_at_ms` is when it will expire.
    Warm { expires_at_ms: Option<i64> },
    /// Prompt cache has expired; `idle_ms` is how long since the newest transcript row if known.
    Expired { idle_ms: Option<i64> },
    /// Not enough information to tell.
    Unknown,
}

/// Resolved canonical agent id for compact-before-sending checks.
fn resolved_agent(state: &ChatState) -> Option<&'static str> {
    crate::extras::agents::transcript_agent([
        state.session.agent.as_deref(),
        state.session.session_agent_id.as_deref(),
    ])
}

/// Whether this agent accepts `/compact` in Ghostex's Compact & Send flow.
pub fn agent_supports_compact(agent: Option<&str>) -> bool {
    matches!(
        agent,
        Some(
            "claude"
                | "openclaude"
                | "codex"
                | "cursor"
                | "grok"
                | "hermes"
                | "copilot"
                | "pi"
                | "omp"
        )
    )
}

/// Whether `offerCompactBeforeSending` is enabled (defaults to `true`).
pub fn is_enabled(state: &ChatState) -> bool {
    state.core.offer_compact_before_sending.unwrap_or(true)
}

/// The active token threshold (`claudeIdleCompactAbove`, default 200K, floor 100K).
pub fn threshold(state: &ChatState) -> u64 {
    state
        .core
        .compact_chats_above
        .unwrap_or(DEFAULT_COMPACT_CHATS_ABOVE)
        .max(MIN_COMPACT_CHATS_ABOVE)
}

/// Timestamp (epoch ms) of the newest transcript row in this chat.
fn newest_transcript_row_at(state: &ChatState) -> Option<i64> {
    state
        .messages
        .composed
        .iter()
        .filter(|row| row.source == ChatSource::Transcript)
        .filter_map(|row| row.timestamp)
        .max()
}

/// Last known context size in tokens from `selectedOptions.contextUsage`.
fn context_tokens(state: &ChatState) -> Option<u64> {
    let usage = state
        .session
        .selected_options
        .as_ref()?
        .get("contextUsage")?;
    let used = usage
        .get("usedTokens")
        .and_then(|v| v.as_u64().or_else(|| v.as_f64().filter(|n| n.is_finite() && *n >= 0.0).map(|n| n.round() as u64)))
        .or_else(|| {
            let pct = usage.get("usedPercentage")?.as_f64()?;
            let win = usage.get("windowSize")?.as_f64()?;
            if pct.is_finite() && win.is_finite() && pct >= 0.0 && win > 0.0 {
                Some(((pct * win) / 100.0).round() as u64)
            } else {
                None
            }
        })?;
    (used > 0).then_some(used)
}

/// Determines whether the chat's prompt cache has expired (or when it will).
fn cache_status(state: &ChatState, agent: &str, now_ms: i64) -> CacheStatus {
    let newest = newest_transcript_row_at(state);
    let idle_ms = newest.map(|ts| now_ms.saturating_sub(ts)).filter(|ms| *ms >= 0);

    if matches!(agent, "claude" | "openclaude") {
        if let Some(cache) = state
            .session
            .selected_options
            .as_ref()
            .and_then(|opts| opts.get("claudeStatus"))
            .and_then(|status| status.get("promptCache"))
        {
            if cache.get("warm").and_then(|v| v.as_bool()) == Some(false) {
                return CacheStatus::Expired { idle_ms };
            }
            if let Some(expires_at_secs) = cache
                .get("expiresAt")
                .and_then(|v| v.as_f64())
                .filter(|n| n.is_finite() && *n > 0.0)
            {
                let expires_at_ms = (expires_at_secs * 1000.0).round() as i64;
                if now_ms >= expires_at_ms {
                    return CacheStatus::Expired { idle_ms };
                }
                return CacheStatus::Warm {
                    expires_at_ms: Some(expires_at_ms),
                };
            }
        }
        return match newest {
            Some(ts) => {
                if now_ms.saturating_sub(ts) > CLAUDE_FALLBACK_CACHE_MS {
                    CacheStatus::Expired { idle_ms }
                } else {
                    CacheStatus::Warm {
                        expires_at_ms: Some(ts.saturating_add(CLAUDE_FALLBACK_CACHE_MS)),
                    }
                }
            }
            None => CacheStatus::Unknown,
        };
    }

    match newest {
        Some(ts) => {
            if now_ms.saturating_sub(ts) > OTHER_AGENT_IDLE_CACHE_MS {
                CacheStatus::Expired { idle_ms }
            } else {
                CacheStatus::Warm {
                    expires_at_ms: Some(ts.saturating_add(OTHER_AGENT_IDLE_CACHE_MS)),
                }
            }
        }
        None => CacheStatus::Unknown,
    }
}

/// "330K" for 330,200 tokens, "1.1M" for 1,120,000 tokens.
fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else {
        format!("{}K", (tokens + 500) / 1_000)
    }
}

/// Concise human idle time, e.g. "45m", "1h 40m", "2d 3h".
fn format_idle_duration(idle_ms: i64) -> Option<String> {
    if idle_ms < 60_000 {
        return None;
    }
    let total_minutes = idle_ms / 60_000;
    let days = total_minutes / (24 * 60);
    let hours = (total_minutes % (24 * 60)) / 60;
    let minutes = total_minutes % 60;
    if days > 0 {
        if hours > 0 {
            Some(format!("{days}d {hours}h"))
        } else {
            Some(format!("{days}d"))
        }
    } else if hours > 0 {
        if minutes > 0 {
            Some(format!("{hours}h {minutes}m"))
        } else {
            Some(format!("{hours}h"))
        }
    } else {
        Some(format!("{minutes}m"))
    }
}

/// Checks everything except cache expiry (returning `(agent, tokens)` when the chat qualifies).
fn eligible_base(state: &ChatState) -> Option<(&'static str, u64)> {
    if !is_enabled(state)
        || state.composer.compact_prompt_dismissed_session
        || crate::session::working::is_working(state)
        || !crate::composer::document::queue(state).capabilities.can_queue
    {
        return None;
    }
    let agent = resolved_agent(state)?;
    if !agent_supports_compact(Some(agent)) {
        return None;
    }
    let tokens = context_tokens(state)?;
    if tokens < threshold(state) {
        return None;
    }
    Some((agent, tokens))
}

/// The offer projected into `Document::compact_before_send`, or `None` when the chat does not
/// qualify for Ask-on-Enter compaction right now.
pub fn compact_before_send(
    state: &ChatState,
    context: &ChatContext,
) -> Option<CompactBeforeSendOffer> {
    let (agent, tokens) = eligible_base(state)?;
    let CacheStatus::Expired { idle_ms } = cache_status(state, agent, context.now_ms as i64) else {
        return None;
    };
    let tokens_label = format_tokens(tokens);
    let idle_label = idle_ms.and_then(format_idle_duration);
    Some(CompactBeforeSendOffer {
        open: state.composer.compact_prompt_open,
        tokens,
        tokens_label,
        idle_label,
        title: "Compact this chat first?".to_string(),
        detail: "This chat is long and its prompt cache has expired. Compacting first shrinks it before sending, which saves usage.".to_string(),
    })
}

/// Opens the confirmation prompt when the user presses Enter or Send on a qualifying chat.
pub fn open(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    if compact_before_send(state, context).is_some() {
        state.composer.compact_prompt_open = true;
        state.core.request_publish();
    }
    Vec::new()
}

/// Closes the confirmation prompt without changing the "Don't ask again" preferences.
pub fn close(state: &mut ChatState) -> Vec<Effect> {
    if state.composer.compact_prompt_open {
        state.composer.compact_prompt_open = false;
        state.core.request_publish();
    }
    Vec::new()
}

/// Handles "Don't ask again": `"session"` suppresses for this chat only; `"always"` turns off
/// `offerCompactBeforeSending` globally via a host action.
pub fn dismiss(state: &mut ChatState, scope: &str) -> Vec<Effect> {
    state.composer.compact_prompt_open = false;
    state.composer.compact_prompt_dismissed_session = true;
    state.core.request_publish();
    if scope == "always" {
        state.core.offer_compact_before_sending_from_host = true;
        state.core.offer_compact_before_sending = Some(false);
        vec![Effect::HostAction {
            action: DISABLE_OFFER_HOST_ACTION.to_string(),
            params: Box::new(json!({})),
        }]
    } else {
        Vec::new()
    }
}

/// Arms the redraw timer for when an open qualifying chat's cache expires, and closes an open
/// prompt if the chat is no longer eligible.
pub fn settle(state: &mut ChatState, context: &ChatContext) {
    if state.core.timer_fired(COMPACT_PROMPT_TIMER) {
        state.core.request_publish();
    }
    let now_ms = context.now_ms as i64;
    let Some((agent, _)) = eligible_base(state) else {
        if state.composer.compact_prompt_open {
            state.composer.compact_prompt_open = false;
            state.core.request_publish();
        }
        state.core.timers.cancel(COMPACT_PROMPT_TIMER);
        return;
    };
    match cache_status(state, agent, now_ms) {
        CacheStatus::Expired { .. } => {
            state.core.timers.cancel(COMPACT_PROMPT_TIMER);
        }
        CacheStatus::Warm { expires_at_ms } => {
            if state.composer.compact_prompt_open {
                state.composer.compact_prompt_open = false;
                state.core.request_publish();
            }
            let due = expires_at_ms
                .map(|at| (at.saturating_add(1)) as f64)
                .filter(|due| *due > context.now_ms);
            match due {
                Some(due) => {
                    state
                        .core
                        .timers
                        .arm(COMPACT_PROMPT_TIMER, context.now_ms, due - context.now_ms)
                }
                None => {
                    state.core.timers.cancel(COMPACT_PROMPT_TIMER);
                }
            }
        }
        CacheStatus::Unknown => {
            if state.composer.compact_prompt_open {
                state.composer.compact_prompt_open = false;
                state.core.request_publish();
            }
            state.core.timers.cancel(COMPACT_PROMPT_TIMER);
        }
    }
}
