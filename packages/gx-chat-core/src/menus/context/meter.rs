//! The context meter under the composer, and where its status line wraps.
//!
//! Ports of `computeNativeChatContext` (`packages/shared/session-chat-controller/
//! native-context.ts`) and of `sessionChatStatusLineReserved` plus `balancedRowStarts`
//! (`packages/shared/session-chat-presentation/status-line-layout.ts`).

use serde_json::{json, Value};

use crate::menus::context::preferences::{
    resolve_context_detail_groups, resolve_starred_context_details, ContextDetailsPreferences,
    RowSelection,
};
use crate::menus::context::rows::ContextDetailSession;
use crate::menus::context::status::{
    resolve_context_detail_status, AgentAccount, ContextDetailsAgent, DetectedOptions,
};
use crate::menus::context::usage::{
    format_context_percentage, format_context_tokens, mask_account_text,
    resolve_context_meter_usage, ContextMeterUsage,
};

/// How long the meter waits before it redraws its countdown labels.
pub const CONTEXT_METER_REFRESH_MS: f64 = 30_000.0;

/// CDXC:SessionChat 2026-09-19 SEE-ALSO:
/// One status-line row, the height the renderer holds free under the chat box from the first
/// frame so the box never shifts when the values arrive.
/// `apps/desktop/src/app/native_chat/context_meter.rs` reserves this height.
pub const STATUS_LINE_ROW_HEIGHT_PX: u32 = 16;

/// `sessionChatStatusLineReserved`: configured items reserve the row while their values are still
/// loading, so the first paint already has room for the line the session is going to show.
///
/// CDXC:AgentProviders 2026-09-14 DECISION:
/// User: show the status line as soon as its values are known, including on the first load.
/// This supersedes the fixed three-second initial wait; configured items still reserve space while loading.
pub fn status_line_reserved(has_configured_items: bool, item_count: usize) -> bool {
    has_configured_items || item_count > 0
}

/// CDXC:SessionChat 2026-09-14 DECISION:
/// User: center every status-line row, avoid a row with only one item where possible, and show
/// separators only between items on the same row. This supersedes the 2026-09-04 container-only
/// centering and left-aligned wrapped rows.
pub fn balanced_row_starts(widths: &[f64], available: f64, separator: f64) -> Vec<u32> {
    #[derive(Clone)]
    struct Layout {
        rows: usize,
        singletons: usize,
        slack: f64,
        starts: Vec<u32>,
    }
    let mut layouts: Vec<Option<Layout>> = vec![Some(Layout {
        rows: 0,
        singletons: 0,
        slack: 0.0,
        starts: Vec::new(),
    })];
    for end in 1..=widths.len() {
        let mut best: Option<Layout> = None;
        let mut width = 0.0;
        for start in (0..end).rev() {
            width += widths[start].min(available) + if start < end - 1 { separator } else { 0.0 };
            if width > available {
                break;
            }
            let Some(previous) = layouts[start].as_ref() else {
                continue;
            };
            let mut starts = previous.starts.clone();
            starts.push(start as u32);
            let candidate = Layout {
                rows: previous.rows + 1,
                singletons: previous.singletons + usize::from(end - start == 1),
                slack: previous.slack + (available - width).powi(2),
                starts,
            };
            let better = match best.as_ref() {
                None => true,
                Some(best) => {
                    candidate.rows < best.rows
                        || (candidate.rows == best.rows
                            && (candidate.singletons < best.singletons
                                || (candidate.singletons == best.singletons
                                    && candidate.slack < best.slack)))
                }
            };
            if better {
                best = Some(candidate);
            }
        }
        layouts.push(best);
    }
    layouts
        .pop()
        .flatten()
        .map(|layout| layout.starts)
        .unwrap_or_default()
}

/// Everything `computeNativeChatContext` needs that family a and e1 own.
pub struct ContextMeterInput<'a> {
    /// The session's agent icon, which chooses the catalog: `codex`, `claude` or another agent.
    pub icon: Option<&'a str>,
    /// `chat.selectedOptions`, folded by family a.
    pub selected_options: Option<&'a DetectedOptions>,
    /// The account assigned to this session, or `None`.
    pub account: Option<&'a AgentAccount>,
    /// `chat.agentSessionId`.
    pub agent_session_id: Option<String>,
    /// `chat.availableAgents !== null`: nothing has reached the agent yet.
    pub draft: bool,
    /// The session's display title.
    pub title: Option<String>,
    /// `chat.working`: Compact is refused while the agent is busy.
    pub working: bool,
    /// Masks account text everywhere the chat shows it.
    ///
    /// CDXC:AgentProviders 2026-09-10 DECISION: Hide emails also applies to the chat status line, context meter details, and Context details dialog previews, including hover text.
    pub hide_account_emails: bool,
}

/// What the meter resolved, so the editor can reuse the same status without rebuilding it.
pub struct ContextMeterResult {
    /// The `contextMeter` document key, `null` when this agent has neither usage nor details.
    pub meter: Value,
    /// Which catalog this session uses.
    pub agent: ContextDetailsAgent,
    /// `resolveContextDetailStatus(agent, selectedOptions, account)`.
    pub status: crate::menus::context::status::ContextDetailStatus,
    /// The session row's own view of the session.
    pub session: ContextDetailSession,
}

/// `computeNativeChatContext`.
pub fn compute_context_meter(
    input: &ContextMeterInput<'_>,
    preferences: &ContextDetailsPreferences,
    context: &crate::ChatContext,
) -> ContextMeterResult {
    let agent = ContextDetailsAgent::from_icon(input.icon);
    let has_details = ContextDetailsAgent::for_icon(input.icon).is_some();
    let reported = resolve_context_meter_usage(
        input
            .selected_options
            .and_then(|options| options.context_usage.as_ref()),
        agent == ContextDetailsAgent::Codex,
    );
    let status = resolve_context_detail_status(agent, input.selected_options, input.account);
    let session = ContextDetailSession {
        title: input.title.clone(),
        agent_session_id: input.agent_session_id.clone(),
        draft: input.draft,
    };
    if reported.is_none() && !has_details {
        return ContextMeterResult {
            meter: Value::Null,
            agent,
            status,
            session,
        };
    }
    let usage = reported.unwrap_or(ContextMeterUsage {
        used_percentage: None,
        used_tokens: None,
        window_size: None,
    });
    let percentage = format_context_percentage(usage.used_percentage);
    let text = |value: &str| {
        if input.hide_account_emails {
            mask_account_text(value)
        } else {
            value.to_string()
        }
    };
    let details = has_details.then(|| {
        resolve_context_detail_groups(
            Some(&status),
            preferences,
            context,
            RowSelection::Shown,
            Some(&session),
            agent,
        )
    });
    let starred = if has_details {
        resolve_starred_context_details(Some(&status), preferences, context, Some(&session), agent)
    } else {
        Vec::new()
    };
    let has_configured_items = has_details && preferences.starred.values().any(|starred| *starred);
    let label = match &percentage {
        Some(percentage) => format!("Context window {percentage} used"),
        None => match usage.used_tokens {
            None => "Context usage not yet reported".to_string(),
            Some(tokens) => format!(
                "Context window {} tokens used",
                format_context_tokens(Some(tokens))
            ),
        },
    };
    let summary = match (usage.window_size, &percentage) {
        (Some(window), Some(percentage)) => format!(
            "{percentage} · {}/{}",
            format_context_tokens(usage.used_tokens),
            format_context_tokens(Some(window))
        ),
        _ => match &percentage {
            Some(percentage) => percentage.clone(),
            None => match usage.used_tokens {
                None => "Not yet reported".to_string(),
                Some(tokens) => format_context_tokens(Some(tokens)),
            },
        },
    };
    let tooltip = input
        .selected_options
        .and_then(|options| options.terminal_status_line.as_deref())
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| label.clone());
    let meter = json!({
        "usedPercentage": usage.used_percentage,
        "usedTokens": usage.used_tokens,
        "windowSize": usage.window_size,
        "percentage": percentage,
        "label": label,
        "summary": summary,
        "hasConfiguredItems": has_configured_items,
        // The native status line holds its row of space by the rule React's `is-reserved` class
        // applied.
        "statusLineReserved": status_line_reserved(has_configured_items, starred.len()),
        "details": match details {
            None => Value::Null,
            Some(groups) => Value::Array(
                groups
                    .iter()
                    .map(|group| {
                        json!({
                            "id": group.id.as_str(),
                            "label": group.id.label(),
                            "items": group
                                .items
                                .iter()
                                .map(|item| {
                                    let mut masked = item.clone();
                                    masked.value = text(&item.value);
                                    masked.to_json()
                                })
                                .collect::<Vec<_>>(),
                        })
                    })
                    .collect(),
            ),
        },
        "starred": starred
            .iter()
            .map(|item| {
                let mut masked = item.clone();
                masked.value = text(&item.value);
                masked.to_json()
            })
            .collect::<Vec<_>>(),
        "tooltip": text(&tooltip),
        "compactDisabled": input.working,
        "compactDisabledReason": input
            .working
            .then(|| "Available once the agent is idle.".to_string()),
    });
    ContextMeterResult {
        meter,
        agent,
        status,
        session,
    }
}
