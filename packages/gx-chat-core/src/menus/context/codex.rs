//! Codex's own context-detail rows.
//!
//! Port of `CODEX_CONTEXT_DETAIL_ROWS` in
//! `packages/core-ui/chat/session-chat-context-details-agents.ts`.

use crate::menus::context::rows::{count, duration, words, GroupId, RowDefinition, RowInput};
use crate::menus::context::status::CodexTokens;
use crate::menus::context::time::{date_parse, to_locale_string};
use crate::menus::picker::js::{js_number, to_fixed};

fn total_tokens(input: &RowInput) -> Option<CodexTokens> {
    input
        .status
        .codex
        .as_ref()
        .and_then(|codex| codex.total_tokens)
}

fn value_total_input_tokens(input: &RowInput) -> Option<String> {
    count(total_tokens(input).and_then(|tokens| tokens.input_tokens))
}

fn value_total_tokens(input: &RowInput) -> Option<String> {
    count(total_tokens(input).and_then(|tokens| tokens.total_tokens))
}

fn value_cached_tokens(input: &RowInput) -> Option<String> {
    count(total_tokens(input).and_then(|tokens| tokens.cached_input_tokens))
}

fn value_cache_write_tokens(input: &RowInput) -> Option<String> {
    count(total_tokens(input).and_then(|tokens| tokens.cache_write_input_tokens))
}

fn value_reasoning_tokens(input: &RowInput) -> Option<String> {
    count(total_tokens(input).and_then(|tokens| tokens.reasoning_output_tokens))
}

fn value_turn_tokens(input: &RowInput) -> Option<String> {
    count(
        input
            .status
            .codex
            .as_ref()
            .and_then(|codex| codex.turn_tokens)
            .and_then(|tokens| tokens.total_tokens),
    )
}

fn value_cache_ratio(input: &RowInput) -> Option<String> {
    let usage = total_tokens(input)?;
    // `usage?.inputTokens && usage.cachedInputTokens !== undefined`: a zero input is falsy.
    let input_tokens = usage.input_tokens.filter(|value| *value != 0.0)?;
    let cached = usage.cached_input_tokens?;
    Some(format!("{}%", to_fixed(cached / input_tokens * 100.0, 1)))
}

/// CDXC:AgentProviders 2026-09-08 DECISION:
/// User: show how many usage resets remain from the saved Codex account in context details and
/// the configurable status line.
fn value_account_resets(input: &RowInput) -> Option<String> {
    let resets = input
        .status
        .account
        .as_ref()
        .and_then(|account| account.reset_credits)?;
    Some(format!(
        "{} {}",
        js_number(resets),
        if resets == 1.0 { "reset" } else { "resets" }
    ))
}

fn value_credits(input: &RowInput) -> Option<String> {
    let credits = input
        .status
        .codex
        .as_ref()
        .and_then(|codex| codex.credits.as_ref());
    if credits.is_some_and(|credits| credits.unlimited == Some(true)) {
        return Some("Unlimited".to_string());
    }
    if let Some(balance) = credits.and_then(|credits| credits.balance.clone()) {
        return Some(balance);
    }
    match credits.and_then(|credits| credits.has_credits) {
        None => None,
        Some(true) => Some("Available".to_string()),
        Some(false) => Some("None".to_string()),
    }
}

fn value_plan(input: &RowInput) -> Option<String> {
    words(
        input
            .status
            .codex
            .as_ref()
            .and_then(|codex| codex.plan.as_ref()),
    )
}

fn value_last_turn_duration(input: &RowInput) -> Option<String> {
    duration(
        input
            .status
            .codex
            .as_ref()
            .and_then(|codex| codex.last_turn_duration_ms),
    )
}

fn value_first_token_time(input: &RowInput) -> Option<String> {
    duration(
        input
            .status
            .codex
            .as_ref()
            .and_then(|codex| codex.time_to_first_token_ms),
    )
}

fn value_provider(input: &RowInput) -> Option<String> {
    input
        .status
        .codex
        .as_ref()
        .and_then(|codex| codex.provider.clone())
}

fn value_sandbox(input: &RowInput) -> Option<String> {
    words(
        input
            .status
            .codex
            .as_ref()
            .and_then(|codex| codex.sandbox.as_ref()),
    )
}

fn value_approval_policy(input: &RowInput) -> Option<String> {
    words(
        input
            .status
            .codex
            .as_ref()
            .and_then(|codex| codex.approval_policy.as_ref()),
    )
}

fn value_parent_thread(input: &RowInput) -> Option<String> {
    let codex = input.status.codex.as_ref()?;
    codex
        .parent_thread_id
        .clone()
        .or_else(|| codex.forked_from_id.clone())
}

fn value_started_at(input: &RowInput) -> Option<String> {
    let stamp = input
        .status
        .codex
        .as_ref()
        .and_then(|codex| codex.started_at.clone())
        .unwrap_or_default();
    let timestamp = date_parse(&stamp)?;
    // The host's own locale rendering when it supplied one for this stamp; the crate's `en-US`
    // fallback otherwise, which is what V8 printed for QuickJS and what the replay reproduced.
    if let Some(text) = input.context.formatted_time(
        crate::FormattedTimeStyle::ContextStartedAt,
        timestamp.round() as i64,
    ) {
        return Some(text.to_string());
    }
    Some(to_locale_string(timestamp, input.utc_offset_minutes()))
}

/// `CODEX_CONTEXT_DETAIL_ROWS`, in catalog order.
pub const CODEX_ROWS: &[RowDefinition] = &[
    RowDefinition {
        id: "totalInputTokens",
        group: GroupId::Context,
        label: "Total input tokens",
        description: "Cumulative input, including cached input",
        recommended: false,
        value: value_total_input_tokens,
        copy: None,
    },
    RowDefinition {
        id: "totalTokens",
        group: GroupId::Context,
        label: "Total tokens",
        description: "Cumulative session usage, distinct from current context",
        recommended: false,
        value: value_total_tokens,
        copy: None,
    },
    RowDefinition {
        id: "cachedTokens",
        group: GroupId::Context,
        label: "Cached input tokens",
        description: "Cumulative cached input, already included in total input",
        recommended: false,
        value: value_cached_tokens,
        copy: None,
    },
    RowDefinition {
        id: "cacheWriteTokens",
        group: GroupId::Context,
        label: "Cache-write tokens",
        description: "Cumulative cache writes, when reported",
        recommended: false,
        value: value_cache_write_tokens,
        copy: None,
    },
    RowDefinition {
        id: "reasoningTokens",
        group: GroupId::Context,
        label: "Reasoning tokens",
        description: "Cumulative reasoning output, already included in output tokens",
        recommended: false,
        value: value_reasoning_tokens,
        copy: None,
    },
    RowDefinition {
        id: "turnTokens",
        group: GroupId::Context,
        label: "Current turn tokens",
        description: "Usage attributed to the latest recorded turn",
        recommended: false,
        value: value_turn_tokens,
        copy: None,
    },
    RowDefinition {
        id: "cacheRatio",
        group: GroupId::Context,
        label: "Cached input share",
        description: "Calculated cached input divided by cumulative input",
        recommended: false,
        value: value_cache_ratio,
        copy: None,
    },
    RowDefinition {
        id: "accountResets",
        group: GroupId::Usage,
        label: "Account resets",
        description: "Available usage resets from the saved account assigned to this session",
        recommended: false,
        value: value_account_resets,
        copy: None,
    },
    RowDefinition {
        id: "credits",
        group: GroupId::Usage,
        label: "Account credits",
        description: "Credit balance reported by Codex, distinct from session cost",
        recommended: false,
        value: value_credits,
        copy: None,
    },
    RowDefinition {
        id: "plan",
        group: GroupId::Usage,
        label: "Account plan",
        description: "Plan last reported by this Codex session",
        recommended: false,
        value: value_plan,
        copy: None,
    },
    RowDefinition {
        id: "lastTurnDuration",
        group: GroupId::Usage,
        label: "Last turn duration",
        description: "Codex-reported duration of the latest completed turn",
        recommended: true,
        value: value_last_turn_duration,
        copy: None,
    },
    RowDefinition {
        id: "firstTokenTime",
        group: GroupId::Usage,
        label: "Time to first token",
        description: "Codex-reported time to first token on the latest completed turn",
        recommended: false,
        value: value_first_token_time,
        copy: None,
    },
    RowDefinition {
        id: "provider",
        group: GroupId::Session,
        label: "Model provider",
        description: "The provider recorded in the Codex session",
        recommended: false,
        value: value_provider,
        copy: None,
    },
    RowDefinition {
        id: "sandbox",
        group: GroupId::Session,
        label: "Sandbox",
        description: "Sandbox mode recorded at turn start",
        recommended: false,
        value: value_sandbox,
        copy: None,
    },
    RowDefinition {
        id: "approvalPolicy",
        group: GroupId::Session,
        label: "Approval policy",
        description: "Approval policy recorded at turn start",
        recommended: false,
        value: value_approval_policy,
        copy: None,
    },
    RowDefinition {
        id: "parentThread",
        group: GroupId::Session,
        label: "Parent thread",
        description: "Parent or fork source recorded by Codex",
        recommended: false,
        value: value_parent_thread,
        copy: None,
    },
    RowDefinition {
        id: "startedAt",
        group: GroupId::Session,
        label: "Started",
        description: "Session creation time recorded by Codex",
        recommended: false,
        value: value_started_at,
        copy: None,
    },
];
