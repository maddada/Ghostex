//! The catalog of context-detail rows.
//!
//! Port of `SESSION_CHAT_CONTEXT_DETAIL_ROWS` and the row definitions in
//! `packages/shared/session-chat-presentation/context-details.ts`, plus `USAGE_WINDOW_ROWS`,
//! `CODEX_CONTEXT_DETAIL_ROWS` and `SHARED_CONTEXT_DETAIL_ROWS` from
//! `packages/core-ui/chat/session-chat-context-details-agents.ts`.
//!
//! CDXC:SessionChatDetectedOptions 2026-09-04 DECISION:
//! User: the context meter popover gets a "More details" section under the Compact button, with
//! rows grouped under "Usage & cost", "Context & cache" and "Session". A pen icon opens a dialog
//! to show/hide rows and reorder them within their group only (rows never cross a group). Any
//! row, shown or not, can be starred, and the starred values render as one wrapping text line
//! under the chat box, each with its title on hover. A group label is never rendered without at
//! least one row under it. The catalog of rows lives here; the popover, the dialog and the status
//! line only render what `resolve_context_detail_groups` returns.
//!
//! CDXC:SessionChatDetectedOptions 2026-09-11 DECISION:
//! User: every row holds one value, so a starred row is one item in the status line. Cost, Prompt
//! cache, Last request, Context used and the Codex Permissions rows were split into one row per
//! value, Remaining context was removed as the inverse of Context used, and Account extra usage
//! left the Codex catalog because Codex accounts never report a spend window. The split rows keep
//! the recommended state of the row they came from.

use crate::menus::context::status::{ContextDetailStatus, ContextDetailsAgent};
use crate::menus::context::time::date_parse;
use crate::menus::context::usage::{
    format_context_tokens, format_duration, format_reset_countdown,
};
use crate::menus::context::windows::{
    account_usage_samples, join, usage_percent_text, usage_reset_countdown, usage_reset_text,
    usage_windows, UsageWindowKind, UsageWindowSample,
};
use crate::menus::picker::js::{is_finite, js_number, js_round, to_fixed};

/// The three groups, in the order the popover draws them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GroupId {
    Usage,
    Context,
    Session,
}

impl GroupId {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Usage => "usage",
            Self::Context => "context",
            Self::Session => "session",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Usage => "Usage & cost",
            Self::Context => "Context & cache",
            Self::Session => "Session",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "usage" => Some(Self::Usage),
            "context" => Some(Self::Context),
            "session" => Some(Self::Session),
            _ => None,
        }
    }
}

/// `SESSION_CHAT_CONTEXT_DETAIL_GROUPS`.
pub const CONTEXT_DETAIL_GROUPS: [GroupId; 3] =
    [GroupId::Usage, GroupId::Context, GroupId::Session];

/// Ghostex's own view of the session for the session row.
///
/// User: the title and id come from Ghostex data (the sidebar title, the agent session id on the
/// chat read state), not from Claude's payload.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContextDetailSession {
    /// The sidebar title, `None` while the session has none.
    pub title: Option<String>,
    /// Claude's conversation id (`claude --resume` takes it), `None` until it resolves.
    pub agent_session_id: Option<String>,
    /// No prompt has reached the agent yet, so the id is not one worth copying.
    pub draft: bool,
}

/// What every row's value function reads.
pub struct RowInput<'a> {
    pub status: &'a ContextDetailStatus,
    /// `None` when the host did not describe the session; the session row is skipped.
    pub session: Option<&'a ContextDetailSession>,
    /// The host's clock, offset and pre-formatted times for this turn.
    ///
    /// The whole context rather than the two numbers the rows read today, so a later input (the
    /// host's locale rendering of a stamp was the first) costs no signature change here or in the
    /// three builders that construct this.
    pub context: &'a crate::ChatContext,
}

impl RowInput<'_> {
    /// Milliseconds since the epoch, for the reset and expiry countdowns.
    pub fn now(&self) -> f64 {
        self.context.now_ms
    }

    /// Only `startedAt` reads it, for `toLocaleString`.
    pub fn utc_offset_minutes(&self) -> i32 {
        self.context.utc_offset_minutes
    }
}

/// Text a click on a status line item copies, with the title of the toast that says so.
pub type RowCopy = fn(&RowInput) -> Option<(String, String)>;

/// One row of the catalog.
#[derive(Clone, Copy)]
pub struct RowDefinition {
    pub id: &'static str,
    pub group: GroupId,
    pub label: &'static str,
    pub description: &'static str,
    /// Shown in the popover on a fresh install. Starred is never a default.
    pub recommended: bool,
    /// `None` when the agent has not reported a value; popovers and the status line omit it.
    pub value: fn(&RowInput) -> Option<String>,
    /// Text a click on the status line item copies, with the toast title.
    pub copy: Option<RowCopy>,
}

impl PartialEq for RowDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.label == other.label && self.description == other.description
    }
}

impl std::fmt::Debug for RowDefinition {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RowDefinition")
            .field("id", &self.id)
            .finish()
    }
}

const SEPARATOR: &str = " · ";

pub(crate) fn join_parts(parts: [Option<String>; 2]) -> Option<String> {
    let present: Vec<String> = parts
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect();
    (!present.is_empty()).then(|| present.join(SEPARATOR))
}

fn format_usd(value: f64) -> String {
    format!("${}", to_fixed(value, 2))
}

/// Time left until an epoch-seconds instant, or `None` once it has passed.
fn format_countdown(epoch_seconds: f64, now: f64) -> Option<String> {
    let remaining_ms = epoch_seconds * 1000.0 - now;
    (remaining_ms > 0.0).then(|| format_reset_countdown(remaining_ms))
}

fn base_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['\\', '/']);
    let name = trimmed.rsplit(['\\', '/']).next().unwrap_or("");
    if name.is_empty() {
        trimmed.to_string()
    } else {
        name.to_string()
    }
}

/// `words`: `value?.replace(/[_-]/g, ' ') ?? null`.
pub(crate) fn words(value: Option<&String>) -> Option<String> {
    value.map(|value| value.replace(['_', '-'], " "))
}

/// `count`: a finite token count, formatted.
pub(crate) fn count(value: Option<f64>) -> Option<String> {
    is_finite(value).then(|| format_context_tokens(value))
}

/// `duration`: a finite millisecond span, formatted.
pub(crate) fn duration(value: Option<f64>) -> Option<String> {
    is_finite(value).then(|| format_duration(value.unwrap_or_default()))
}

// ---------------------------------------------------------------------------
// Usage & cost

fn value_cost_usd(input: &RowInput) -> Option<String> {
    let total = input.status.cost.and_then(|cost| cost.total_usd);
    is_finite(total).then(|| format_usd(total.unwrap_or_default()))
}

fn value_session_time(input: &RowInput) -> Option<String> {
    duration(input.status.cost.and_then(|cost| cost.duration_ms))
}

fn value_api_time(input: &RowInput) -> Option<String> {
    let api = input.status.cost.and_then(|cost| cost.api_duration_ms);
    is_finite(api).then(|| format!("API {}", format_duration(api.unwrap_or_default())))
}

fn value_lines(input: &RowInput) -> Option<String> {
    let added = input.status.cost.and_then(|cost| cost.lines_added);
    let removed = input.status.cost.and_then(|cost| cost.lines_removed);
    (is_finite(added) || is_finite(removed)).then(|| {
        format!(
            "+{} / \u{2212}{}",
            js_number(added.unwrap_or(0.0)),
            js_number(removed.unwrap_or(0.0))
        )
    })
}

fn usage_row_value(
    input: &RowInput,
    kind: UsageWindowKind,
    render: fn(&UsageWindowSample, f64) -> Option<String>,
) -> Option<String> {
    join(
        usage_windows(input.status, kind, input.now())
            .iter()
            .map(|window| render(window, input.now()))
            .collect::<Vec<_>>(),
    )
}

fn value_five_hour_limit(input: &RowInput) -> Option<String> {
    usage_row_value(input, UsageWindowKind::FiveHour, |window, _| {
        usage_percent_text(window)
    })
}

fn value_seven_day_limit(input: &RowInput) -> Option<String> {
    usage_row_value(input, UsageWindowKind::SevenDay, |window, _| {
        usage_percent_text(window)
    })
}

fn value_model_limit(input: &RowInput) -> Option<String> {
    usage_row_value(input, UsageWindowKind::Model, |window, _| {
        usage_percent_text(window)
    })
}

fn value_five_hour_reset(input: &RowInput) -> Option<String> {
    usage_row_value(input, UsageWindowKind::FiveHour, usage_reset_text)
}

fn value_seven_day_reset(input: &RowInput) -> Option<String> {
    usage_row_value(input, UsageWindowKind::SevenDay, usage_reset_text)
}

// ---------------------------------------------------------------------------
// Context & cache

fn value_cache_state(input: &RowInput) -> Option<String> {
    let warm = input
        .status
        .prompt_cache
        .as_ref()
        .and_then(|cache| cache.warm)?;
    Some(if warm { "cache warm" } else { "cache cold" }.to_string())
}

fn value_cache_time_left(input: &RowInput) -> Option<String> {
    let cache = input.status.prompt_cache.as_ref()?;
    if cache.warm != Some(true) || !is_finite(cache.expires_at) {
        return None;
    }
    let left = format_countdown(cache.expires_at.unwrap_or_default(), input.now())?;
    Some(format!("{left} left"))
}

fn value_cache_hit_rate(input: &RowInput) -> Option<String> {
    let ratio = input
        .status
        .prompt_cache
        .as_ref()
        .and_then(|cache| cache.hit_ratio);
    is_finite(ratio).then(|| {
        format!(
            "{}% hits",
            js_number(js_round(ratio.unwrap_or_default() * 100.0))
        )
    })
}

/// `lastRequestRow`: a last-request token count keeps its role word (12k in) because the row
/// stands alone in the status line.
fn last_request_value(
    input: &RowInput,
    field: fn(&crate::menus::context::status::LastRequestTokens) -> Option<f64>,
    suffix: &str,
) -> Option<String> {
    let tokens = input.status.last_request.as_ref().and_then(field);
    is_finite(tokens).then(|| format!("{} {suffix}", format_context_tokens(tokens)))
}

fn value_last_request_input(input: &RowInput) -> Option<String> {
    last_request_value(input, |tokens| tokens.input_tokens, "in")
}

fn value_last_request_output(input: &RowInput) -> Option<String> {
    last_request_value(input, |tokens| tokens.output_tokens, "out")
}

fn value_last_request_cached(input: &RowInput) -> Option<String> {
    last_request_value(input, |tokens| tokens.cache_read_tokens, "cached")
}

fn value_last_request_cache_write(input: &RowInput) -> Option<String> {
    last_request_value(input, |tokens| tokens.cache_write_tokens, "cache writes")
}

fn value_total_output_tokens(input: &RowInput) -> Option<String> {
    let total = input.status.total_output_tokens;
    is_finite(total).then(|| format_context_tokens(total))
}

fn value_cache_misses(input: &RowInput) -> Option<String> {
    let misses = input
        .status
        .prompt_cache
        .as_ref()
        .and_then(|cache| cache.misses);
    is_finite(misses).then(|| {
        let misses = misses.unwrap_or_default();
        format!(
            "{} {}",
            js_number(misses),
            if misses == 1.0 { "miss" } else { "misses" }
        )
    })
}

fn value_cache_last_miss(input: &RowInput) -> Option<String> {
    input
        .status
        .prompt_cache
        .as_ref()
        .and_then(|cache| cache.last_miss_cause.clone())
        .filter(|cause| !cause.is_empty())
}

// ---------------------------------------------------------------------------
// Session

fn value_thinking(input: &RowInput) -> Option<String> {
    let enabled = input.status.thinking_enabled?;
    Some(if enabled { "on" } else { "off" }.to_string())
}

/// The Codex override of `thinking`.
fn value_effort_name(input: &RowInput) -> Option<String> {
    input.status.effort_name.clone()
}

fn value_version(input: &RowInput) -> Option<String> {
    input.status.version.clone()
}

fn value_output_style(input: &RowInput) -> Option<String> {
    input.status.output_style.clone()
}

/// CDXC:SessionChatDetectedOptions 2026-11-06 DECISION:
/// User: the Session group gets an Agent row with the name of the agent the session runs as
/// (Claude Code's `--agent`, the status line's `agent.name`), off until turned on and starrable
/// like the others. A session with no named agent shows nothing.
fn value_agent_name(input: &RowInput) -> Option<String> {
    input
        .status
        .agent_name
        .clone()
        .filter(|name| !name.is_empty())
}

/// User: the id stands in until the session has a title, and a draft (nothing sent yet) says so
/// instead of showing an id that will not be resumed.
fn value_session_name(input: &RowInput) -> Option<String> {
    let session = input.session?;
    if session.draft {
        return Some("Draft session".to_string());
    }
    session
        .title
        .clone()
        .or_else(|| session.agent_session_id.clone())
}

/// User: clicking the session name in the status line copies the session id.
fn copy_session_name(input: &RowInput) -> Option<(String, String)> {
    let session = input.session?;
    if session.draft {
        return None;
    }
    let id = session.agent_session_id.clone()?;
    Some((id, "Session id copied".to_string()))
}

fn value_repo(input: &RowInput) -> Option<String> {
    let repo = input.status.repo.as_ref()?;
    let name = repo.name.as_deref().filter(|name| !name.is_empty())?;
    match repo.owner.as_deref().filter(|owner| !owner.is_empty()) {
        Some(owner) => Some(format!("{owner}/{name}")),
        None => Some(name.to_string()),
    }
}

fn value_folder(input: &RowInput) -> Option<String> {
    let dir = input
        .status
        .current_dir
        .clone()
        .or_else(|| input.status.project_dir.clone())
        .filter(|dir| !dir.is_empty())?;
    Some(format!("…/{}", base_name(&dir)))
}

fn value_pr(input: &RowInput) -> Option<String> {
    let pr = input.status.pr.as_ref()?;
    if !is_finite(pr.number) {
        return None;
    }
    join_parts([
        Some(format!("#{}", js_number(pr.number.unwrap_or_default()))),
        pr.review_state
            .as_deref()
            .filter(|state| !state.is_empty())
            .map(|state| state.replace('_', " ").to_lowercase()),
    ])
}

// ---------------------------------------------------------------------------
// Shared rows, selectable in both agents' popovers and status lines.

fn value_context_used(input: &RowInput) -> Option<String> {
    input.status.context_used_percent.clone()
}

fn value_context_tokens(input: &RowInput) -> Option<String> {
    input.status.context_tokens.clone()
}

fn value_model(input: &RowInput) -> Option<String> {
    input.status.model_name.clone()
}

fn value_account_name(input: &RowInput) -> Option<String> {
    input
        .status
        .account
        .as_ref()
        .map(|account| account.name.clone())
}

fn value_account_email(input: &RowInput) -> Option<String> {
    input
        .status
        .account
        .as_ref()
        .map(|account| account.email.clone())
        .filter(|email| !email.is_empty())
}

fn value_account_spending(input: &RowInput) -> Option<String> {
    let account = input.status.account.as_ref()?;
    join(
        account_usage_samples(account, UsageWindowKind::Spend)
            .iter()
            .map(|window| {
                join_parts([
                    usage_percent_text(window),
                    usage_reset_countdown(window, input.now()),
                ])
            })
            .collect::<Vec<_>>(),
    )
}

fn value_account_usage_updated(input: &RowInput) -> Option<String> {
    let updated = input
        .status
        .account
        .as_ref()
        .and_then(|account| account.usage_updated_at.as_deref())
        .filter(|stamp| !stamp.is_empty())
        .and_then(date_parse)?;
    Some(format!("{} ago", format_duration(input.now() - updated)))
}

fn value_account_usage_status(input: &RowInput) -> Option<String> {
    let account = input.status.account.as_ref();
    account
        .and_then(|account| account.usage_error.clone())
        .or_else(|| words(account.map(|account| &account.status)))
}

fn value_account_sessions(input: &RowInput) -> Option<String> {
    input
        .status
        .account
        .as_ref()
        .map(|account| js_number(account.session_count))
}

/// `USAGE_WINDOW_ROWS`.
///
/// CDXC:AgentProviders 2026-09-11 DECISION:
/// User: the usage rows are one value each, separately for Claude and Codex: 7d limit, 5h limit,
/// the model limit (Fable), 7d reset and 5h reset. This replaced the combined "Rate limits" row
/// and the duplicated account rows, whose values repeated each other. The catalog is static, so
/// every model-scoped window shares the one "Model limit" row. The five-hour and weekly rows are
/// recommended because the retired "Rate limits" row showed exactly those values by default.
const USAGE_WINDOW_ROWS: &[RowDefinition] = &[
    RowDefinition {
        id: "fiveHourLimit",
        group: GroupId::Usage,
        label: "5h limit",
        description: "5h usage from the saved account, or the session when unlinked",
        recommended: true,
        value: value_five_hour_limit,
        copy: None,
    },
    RowDefinition {
        id: "sevenDayLimit",
        group: GroupId::Usage,
        label: "7d limit",
        description: "7d usage from the saved account, or the session when unlinked",
        recommended: true,
        value: value_seven_day_limit,
        copy: None,
    },
    RowDefinition {
        id: "modelLimit",
        group: GroupId::Usage,
        label: "Model limit",
        description: "Model-specific usage from the saved account, such as Fable",
        recommended: false,
        value: value_model_limit,
        copy: None,
    },
    RowDefinition {
        id: "fiveHourReset",
        group: GroupId::Usage,
        label: "5h reset",
        description: "When the 5h window resets",
        recommended: true,
        value: value_five_hour_reset,
        copy: None,
    },
    RowDefinition {
        id: "sevenDayReset",
        group: GroupId::Usage,
        label: "7d reset",
        description: "When the 7d window resets",
        recommended: false,
        value: value_seven_day_reset,
        copy: None,
    },
];

/// `SHARED_CONTEXT_DETAIL_ROWS`.
///
/// CDXC:AgentProviders 2026-09-08 DECISION:
/// User: saved cswap/xswap account stats are selectable in both agents' popovers and status
/// lines. The chat reuses its existing account snapshot and follows the session's assigned
/// account.
const SHARED_ROWS: &[RowDefinition] = &[
    RowDefinition {
        id: "contextUsed",
        group: GroupId::Context,
        label: "Context used",
        description: "Share of the context window in use",
        recommended: false,
        value: value_context_used,
        copy: None,
    },
    RowDefinition {
        id: "contextTokens",
        group: GroupId::Context,
        label: "Context tokens",
        description: "Tokens in use out of the context window",
        recommended: false,
        value: value_context_tokens,
        copy: None,
    },
    RowDefinition {
        id: "model",
        group: GroupId::Session,
        label: "Model",
        description: "The session’s reported model",
        recommended: false,
        value: value_model,
        copy: None,
    },
    RowDefinition {
        id: "accountName",
        group: GroupId::Session,
        label: "Account",
        description: "Saved account assigned to this session",
        recommended: false,
        value: value_account_name,
        copy: None,
    },
    RowDefinition {
        id: "accountEmail",
        group: GroupId::Session,
        label: "Account email",
        description: "Email of the saved account assigned to this session",
        recommended: false,
        value: value_account_email,
        copy: None,
    },
    RowDefinition {
        id: "accountSpending",
        group: GroupId::Usage,
        label: "Account extra usage",
        description: "Account-wide extra spending allowance, distinct from session cost",
        recommended: false,
        value: value_account_spending,
        copy: None,
    },
    RowDefinition {
        id: "accountUsageUpdated",
        group: GroupId::Usage,
        label: "Account usage updated",
        description: "Age of the saved account usage snapshot",
        recommended: false,
        value: value_account_usage_updated,
        copy: None,
    },
    RowDefinition {
        id: "accountUsageStatus",
        group: GroupId::Usage,
        label: "Account usage status",
        description: "Saved account availability or usage refresh error",
        recommended: false,
        value: value_account_usage_status,
        copy: None,
    },
    RowDefinition {
        id: "accountSessions",
        group: GroupId::Session,
        label: "Account sessions",
        description: "Number of Ghostex sessions assigned to this saved account",
        recommended: false,
        value: value_account_sessions,
        copy: None,
    },
];

/// `SESSION_CHAT_CONTEXT_DETAIL_ROWS`, Claude's catalog.
pub fn claude_rows() -> Vec<RowDefinition> {
    let mut rows = vec![
        RowDefinition {
            id: "costUsd",
            group: GroupId::Usage,
            label: "Cost",
            description: "Total spend this session",
            recommended: true,
            value: value_cost_usd,
            copy: None,
        },
        RowDefinition {
            id: "sessionTime",
            group: GroupId::Usage,
            label: "Session time",
            description: "Wall-clock time since the session started",
            recommended: true,
            value: value_session_time,
            copy: None,
        },
        RowDefinition {
            id: "apiTime",
            group: GroupId::Usage,
            label: "API time",
            description: "Time spent waiting on the model",
            recommended: true,
            value: value_api_time,
            copy: None,
        },
    ];
    rows.extend_from_slice(USAGE_WINDOW_ROWS);
    rows.extend_from_slice(&[
        RowDefinition {
            id: "lines",
            group: GroupId::Usage,
            label: "Lines changed",
            description: "Added and removed this session",
            recommended: true,
            value: value_lines,
            copy: None,
        },
        RowDefinition {
            id: "cacheState",
            group: GroupId::Context,
            label: "Cache state",
            description: "Whether the prompt cache is warm or cold",
            recommended: true,
            value: value_cache_state,
            copy: None,
        },
        RowDefinition {
            id: "cacheTimeLeft",
            group: GroupId::Context,
            label: "Cache time left",
            description: "Time before a warm prompt cache expires",
            recommended: true,
            value: value_cache_time_left,
            copy: None,
        },
        RowDefinition {
            id: "cacheHitRate",
            group: GroupId::Context,
            label: "Cache hit rate",
            description: "Share of requests served from the prompt cache",
            recommended: true,
            value: value_cache_hit_rate,
            copy: None,
        },
        RowDefinition {
            id: "lastRequestInput",
            group: GroupId::Context,
            label: "Last request input",
            description: "Input tokens of the latest request",
            recommended: true,
            value: value_last_request_input,
            copy: None,
        },
        RowDefinition {
            id: "lastRequestOutput",
            group: GroupId::Context,
            label: "Last request output",
            description: "Output tokens of the latest request",
            recommended: true,
            value: value_last_request_output,
            copy: None,
        },
        RowDefinition {
            id: "lastRequestCached",
            group: GroupId::Context,
            label: "Last request cached",
            description: "Tokens the latest request read from the cache",
            recommended: true,
            value: value_last_request_cached,
            copy: None,
        },
        RowDefinition {
            id: "lastRequestCacheWrite",
            group: GroupId::Context,
            label: "Last request cache writes",
            description: "Tokens the latest request wrote to the cache",
            recommended: false,
            value: value_last_request_cache_write,
            copy: None,
        },
        RowDefinition {
            id: "totalOutputTokens",
            group: GroupId::Context,
            label: "Total output tokens",
            description: "Everything Claude wrote this session",
            recommended: false,
            value: value_total_output_tokens,
            copy: None,
        },
        RowDefinition {
            id: "cacheMisses",
            group: GroupId::Context,
            label: "Cache misses",
            description: "Prompt cache misses this session",
            recommended: false,
            value: value_cache_misses,
            copy: None,
        },
        RowDefinition {
            id: "cacheLastMiss",
            group: GroupId::Context,
            label: "Last cache miss",
            description: "Cause of the most recent prompt cache miss",
            recommended: false,
            value: value_cache_last_miss,
            copy: None,
        },
        RowDefinition {
            id: "agentName",
            group: GroupId::Session,
            label: "Agent",
            description: "The agent this session runs, from --agent",
            recommended: false,
            value: value_agent_name,
            copy: None,
        },
        RowDefinition {
            id: "thinking",
            group: GroupId::Session,
            label: "Thinking",
            description: "Whether extended thinking is on",
            recommended: true,
            value: value_thinking,
            copy: None,
        },
        RowDefinition {
            id: "version",
            group: GroupId::Session,
            label: "Claude Code version",
            description: "The CLI build running this session",
            recommended: true,
            value: value_version,
            copy: None,
        },
        RowDefinition {
            id: "outputStyle",
            group: GroupId::Session,
            label: "Output style",
            description: "Claude's active output style",
            recommended: false,
            value: value_output_style,
            copy: None,
        },
        RowDefinition {
            id: "sessionName",
            group: GroupId::Session,
            label: "Session title",
            description: "The sidebar title, or the session id until there is one",
            recommended: false,
            value: value_session_name,
            copy: Some(copy_session_name),
        },
        RowDefinition {
            id: "repo",
            group: GroupId::Session,
            label: "Repository",
            description: "Owner and name of the git repository",
            recommended: false,
            value: value_repo,
            copy: None,
        },
        RowDefinition {
            id: "folder",
            group: GroupId::Session,
            label: "Folder",
            description: "Claude's current working folder",
            recommended: false,
            value: value_folder,
            copy: None,
        },
        RowDefinition {
            id: "pr",
            group: GroupId::Session,
            label: "Pull request",
            description: "Number and review state, when one exists",
            recommended: false,
            value: value_pr,
            copy: None,
        },
    ]);
    rows.extend_from_slice(SHARED_ROWS);
    rows
}

/// `CODEX_SHARED_ROWS`: which of Claude's rows the Codex catalog keeps.
fn codex_keeps(id: &str) -> bool {
    matches!(
        id,
        "lastRequestInput"
            | "lastRequestOutput"
            | "lastRequestCached"
            | "lastRequestCacheWrite"
            | "totalOutputTokens"
            | "version"
            | "sessionName"
            | "folder"
            | "thinking"
    ) || USAGE_WINDOW_ROWS.iter().any(|row| row.id == id)
        // Codex accounts report no spend window, so the extra-usage row would always be empty.
        || (SHARED_ROWS.iter().any(|row| row.id == id) && id != "accountSpending")
}

/// `CODEX_ROWS`: Claude's kept rows with their Codex wording, then Codex's own.
pub fn codex_rows() -> Vec<RowDefinition> {
    let mut rows: Vec<RowDefinition> = claude_rows()
        .into_iter()
        .filter(|row| codex_keeps(row.id))
        .map(|row| match row.id {
            "thinking" => RowDefinition {
                label: "Reasoning effort",
                description: "The session’s reasoning effort",
                value: value_effort_name,
                ..row
            },
            "version" => RowDefinition {
                label: "Codex version",
                ..row
            },
            "folder" => RowDefinition {
                description: "Codex's current working folder",
                ..row
            },
            "totalOutputTokens" => RowDefinition {
                description: "Cumulative Codex output, including reasoning tokens",
                ..row
            },
            _ => row,
        })
        .collect();
    rows.extend_from_slice(crate::menus::context::codex::CODEX_ROWS);
    rows
}

/// `CURSOR_SHARED_ROWS`: Claude's rows Cursor also fills, from its statusline payload and the
/// checkout's git state.
fn cursor_keeps(id: &str) -> bool {
    matches!(
        id,
        "lines"
            | "contextUsed"
            | "contextTokens"
            | "totalOutputTokens"
            | "model"
            | "thinking"
            | "version"
            | "sessionName"
            | "repo"
            | "folder"
            | "pr"
    )
}

/// `CURSOR_ROWS`: the shared rows in Cursor's wording, then Cursor's own.
pub fn cursor_rows() -> Vec<RowDefinition> {
    let mut rows: Vec<RowDefinition> = claude_rows()
        .into_iter()
        .filter(|row| cursor_keeps(row.id))
        .map(|row| match row.id {
            "thinking" => RowDefinition {
                label: "Reasoning effort",
                description: "The session’s reasoning effort",
                value: value_effort_name,
                ..row
            },
            "version" => RowDefinition {
                label: "Cursor version",
                ..row
            },
            "folder" => RowDefinition {
                description: "Cursor's current working folder",
                ..row
            },
            "totalOutputTokens" => RowDefinition {
                description: "Everything Cursor wrote this session",
                ..row
            },
            "lines" => RowDefinition {
                description: "Added and removed on this branch",
                ..row
            },
            "repo" => RowDefinition {
                description: "The name of the project folder",
                ..row
            },
            _ => row,
        })
        .collect();
    rows.extend_from_slice(CURSOR_ROWS);
    rows
}

fn value_branch(input: &RowInput) -> Option<String> {
    match input.status.cursor.as_ref() {
        Some(cursor) => cursor.branch.clone(),
        None => input.status.branch.clone(),
    }
}

/// The Basic catalog: the shared rows any session can fill from its checkout and the chat, plus
/// the branch.
pub fn basic_rows() -> Vec<RowDefinition> {
    let mut rows: Vec<RowDefinition> = claude_rows()
        .into_iter()
        .filter(|row| matches!(row.id, "repo" | "folder" | "model" | "sessionName"))
        .map(|row| match row.id {
            "repo" => RowDefinition {
                description: "The name of the project folder",
                recommended: true,
                ..row
            },
            "folder" => RowDefinition {
                description: "The session's working folder",
                ..row
            },
            _ => row,
        })
        .collect();
    rows.extend(CURSOR_ROWS.iter().filter(|row| row.id == "branch").cloned());
    rows
}

fn value_worktree(input: &RowInput) -> Option<String> {
    input.status.cursor.as_ref()?.worktree.clone()
}

fn value_max_mode(input: &RowInput) -> Option<String> {
    let on = input.status.cursor.as_ref()?.max_mode?;
    Some(if on { "Max Mode on" } else { "Max Mode off" }.to_string())
}

fn value_autorun(input: &RowInput) -> Option<String> {
    let on = input.status.cursor.as_ref()?.autorun?;
    Some(if on { "auto-run on" } else { "auto-run off" }.to_string())
}

/// `CURSOR_CONTEXT_DETAIL_ROWS`.
///
/// CDXC:SessionChatDetectedOptions 2026-09-24 DECISION:
/// User: Cursor's rows come from what Cursor hands its statusline command plus the checkout's git
/// state, never only the model and effort. Values carry their own words (Max Mode on, auto-run
/// off) because a starred row stands alone in the status line.
const CURSOR_ROWS: &[RowDefinition] = &[
    RowDefinition {
        id: "branch",
        group: GroupId::Session,
        label: "Branch",
        description: "The checked-out git branch",
        recommended: true,
        value: value_branch,
        copy: None,
    },
    RowDefinition {
        id: "worktree",
        group: GroupId::Session,
        label: "Worktree",
        description: "The Cursor worktree this session runs in",
        recommended: true,
        value: value_worktree,
        copy: None,
    },
    RowDefinition {
        id: "maxMode",
        group: GroupId::Session,
        label: "Max Mode",
        description: "Whether Max Mode is on",
        recommended: true,
        value: value_max_mode,
        copy: None,
    },
    RowDefinition {
        id: "autorun",
        group: GroupId::Session,
        label: "Auto-run",
        description: "Whether Cursor runs commands without asking",
        recommended: true,
        value: value_autorun,
        copy: None,
    },
];

/// `HERMES_SHARED_ROWS`: Claude's rows Hermes fills, from its status line and its session store.
fn hermes_keeps(id: &str) -> bool {
    matches!(id, "contextUsed" | "costUsd" | "sessionTime" | "model")
}

/// `HERMES_ROWS`: the shared rows in Hermes' wording, then Hermes' own.
///
/// CDXC:AgentProviders 2026-09-26 DECISION:
/// User: Hermes' Context details offers only the rows Hermes can back (context used, cost, tokens,
/// session time, model); the 5h and 7d limit rows stay Claude-only.
pub fn hermes_rows() -> Vec<RowDefinition> {
    let mut rows: Vec<RowDefinition> = claude_rows()
        .into_iter()
        .filter(|row| hermes_keeps(row.id))
        .map(|row| match row.id {
            "costUsd" => RowDefinition {
                description: "Spend this session, as Hermes billed or estimated it",
                ..row
            },
            "sessionTime" => RowDefinition {
                description: "Time since the Hermes session started",
                value: value_hermes_session_time,
                ..row
            },
            _ => row,
        })
        .collect();
    rows.extend_from_slice(HERMES_ROWS);
    rows
}

fn value_hermes_tokens(input: &RowInput) -> Option<String> {
    let hermes = input.status.hermes.as_ref()?;
    // Hermes counts cached input apart from `input_tokens`; its own prompt total adds it back.
    let prompt = [
        hermes.input_tokens,
        hermes.cache_read_tokens,
        hermes.cache_write_tokens,
    ]
    .into_iter()
    .flatten()
    .reduce(|total, tokens| total + tokens);
    join_parts([
        count(prompt).map(|tokens| format!("in {tokens}")),
        count(hermes.output_tokens).map(|tokens| format!("out {tokens}")),
    ])
}

fn value_hermes_session_time(input: &RowInput) -> Option<String> {
    let hermes = input.status.hermes.as_ref()?;
    let started = hermes.started_at.filter(|started| started.is_finite())?;
    let ended = hermes
        .ended_at
        .filter(|ended| ended.is_finite())
        .unwrap_or(input.now() / 1000.0);
    duration(Some((ended - started).max(0.0) * 1000.0))
}

const HERMES_ROWS: &[RowDefinition] = &[RowDefinition {
    id: "tokens",
    group: GroupId::Usage,
    label: "Tokens",
    description: "Input and output tokens this session",
    recommended: true,
    value: value_hermes_tokens,
    copy: None,
}];

/// `PI_SHARED_ROWS`: Claude's rows Pi fills, from its transcript and the checkout's git state.
fn pi_keeps(id: &str) -> bool {
    matches!(
        id,
        "contextUsed"
            | "contextTokens"
            | "costUsd"
            | "sessionTime"
            | "totalOutputTokens"
            | "model"
            | "thinking"
            | "sessionName"
            | "repo"
            | "folder"
    )
}

/// `PI_ROWS`: the shared rows in Pi's wording, then the branch and the session's tokens.
///
/// CDXC:AgentProviders 2026-10-06 DECISION:
/// User: Pi's status line shows "model, usage, context, like Claude's", so its Context details offers every row Pi's transcript backs (context, cost, tokens, session time, model, thinking level) plus the checkout's repository, folder and branch; Claude's limit, account and version rows stay Claude-only.
pub fn pi_rows() -> Vec<RowDefinition> {
    let mut rows: Vec<RowDefinition> = claude_rows()
        .into_iter()
        .filter(|row| pi_keeps(row.id))
        .map(|row| match row.id {
            "thinking" => RowDefinition {
                label: "Thinking level",
                description: "The session’s thinking level",
                value: value_effort_name,
                ..row
            },
            "costUsd" => RowDefinition {
                description: "Spend this session, as Pi priced it",
                ..row
            },
            "sessionTime" => RowDefinition {
                description: "Time since the Pi session started",
                value: value_pi_session_time,
                ..row
            },
            "totalOutputTokens" => RowDefinition {
                description: "Everything Pi's models wrote this session",
                ..row
            },
            "folder" => RowDefinition {
                description: "The session's working folder",
                ..row
            },
            "repo" => RowDefinition {
                description: "The name of the project folder",
                ..row
            },
            _ => row,
        })
        .collect();
    rows.extend(CURSOR_ROWS.iter().filter(|row| row.id == "branch").cloned());
    rows.extend_from_slice(PI_ROWS);
    rows
}

fn value_pi_tokens(input: &RowInput) -> Option<String> {
    let pi = input.status.pi.as_ref()?;
    // Pi counts cached input apart from `input`; the prompt total adds it back.
    let prompt = [pi.input_tokens, pi.cache_read_tokens, pi.cache_write_tokens]
        .into_iter()
        .flatten()
        .reduce(|total, tokens| total + tokens);
    join_parts([
        count(prompt).map(|tokens| format!("in {tokens}")),
        count(pi.output_tokens).map(|tokens| format!("out {tokens}")),
    ])
}

fn value_pi_session_time(input: &RowInput) -> Option<String> {
    let started = input
        .status
        .pi
        .as_ref()?
        .started_at
        .filter(|started| started.is_finite())?;
    duration(Some((input.now() - started).max(0.0)))
}

const PI_ROWS: &[RowDefinition] = &[RowDefinition {
    id: "tokens",
    group: GroupId::Usage,
    label: "Tokens",
    description: "Input and output tokens this session",
    recommended: true,
    value: value_pi_tokens,
    copy: None,
}];

/// The catalog for one agent.
pub fn context_detail_rows(agent: ContextDetailsAgent) -> Vec<RowDefinition> {
    match agent {
        ContextDetailsAgent::Codex => codex_rows(),
        ContextDetailsAgent::Claude => claude_rows(),
        ContextDetailsAgent::Cursor => cursor_rows(),
        ContextDetailsAgent::Hermes => hermes_rows(),
        ContextDetailsAgent::Pi => pi_rows(),
        ContextDetailsAgent::Basic => basic_rows(),
    }
}

/// One row of the catalog by id.
pub fn context_detail_row(agent: ContextDetailsAgent, id: &str) -> Option<RowDefinition> {
    context_detail_rows(agent)
        .into_iter()
        .find(|row| row.id == id)
}

/// `isRowId`.
pub fn is_row_id(value: &str, agent: ContextDetailsAgent) -> bool {
    context_detail_rows(agent).iter().any(|row| row.id == value)
}
