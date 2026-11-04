//! What the agent reported, as the context rows read it.
//!
//! Port of the types and of `resolveContextDetailStatus` from
//! `packages/core-ui/chat/session-chat-context-details-agents.ts`. Every field is optional, as it
//! is on the wire, because "the agent has not reported it" is the common case and each row turns
//! that into `null` rather than a placeholder.

use serde::{Deserialize, Serialize};

use crate::menus::context::usage::{
    format_context_percentage, format_context_tokens, resolve_context_meter_usage, ContextUsage,
};

/// Which agent's catalog and preferences a session uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContextDetailsAgent {
    #[default]
    Claude,
    Codex,
    Cursor,
    Hermes,
    Pi,
    /// Every other chat agent: the checkout's repository and branch, which gxserver reads for any
    /// session, plus the folder, model and session title.
    Basic,
}

impl ContextDetailsAgent {
    /// Every agent with its own catalog and saved record.
    pub const ALL: [Self; 6] = [
        Self::Claude,
        Self::Codex,
        Self::Cursor,
        Self::Hermes,
        Self::Pi,
        Self::Basic,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Cursor => "cursor",
            Self::Hermes => "hermes",
            Self::Pi => "pi",
            Self::Basic => "basic",
        }
    }

    /// `contextDetailsAgentFor`: the agent an icon selects, `None` for an agent with no context
    /// details at all.
    ///
    /// CDXC:AgentProviders 2026-09-26 DECISION:
    /// User: Cursor and Hermes chats get their own status line and More details, saved separately
    /// from Claude and Codex, built from what each reports: Cursor its model, reasoning effort and
    /// context use, Hermes its status line and session store. This adds Hermes to the 2026-09-23
    /// Cursor decision.
    ///
    /// CDXC:AgentProviders 2026-10-04 DECISION:
    /// User: "for freebuff just show the repo and branch in the status line (do the same for ALL agents that we have chat support for but we don't actually show anything useful for their status line". Every other chat agent gets the Basic catalog, with repository and branch in its status line.
    ///
    /// CDXC:AgentProviders 2026-10-06 DECISION:
    /// User: Pi gets "Ghostex's own status line for Pi (model, usage, context, like Claude's)". Pi leaves the Basic catalog for its own, saved separately, built from what gxserver reads from the session's Pi transcript (`piStatus`) plus the checkout's repository and branch.
    pub fn for_icon(icon: Option<&str>) -> Option<Self> {
        match icon.map(|icon| icon.trim().to_lowercase()).as_deref() {
            Some("claude") => Some(Self::Claude),
            Some("codex") => Some(Self::Codex),
            Some("cursor" | "cursor-cli" | "cursor cli" | "cursor-agent") => Some(Self::Cursor),
            Some("hermes" | "hermes-agent") => Some(Self::Hermes),
            Some("pi") => Some(Self::Pi),
            Some(
                "antigravity" | "antigravity-cli" | "agy" | "copilot" | "dsh" | "empryo"
                | "freebuff" | "grok" | "grok-build" | "omp" | "opencode" | "openclaude" | "zcode"
                | "zcode-cli",
            ) => Some(Self::Basic),
            _ => None,
        }
    }

    /// Whether the meter shows the percentage the agent printed rather than tokens over window:
    /// Codex adjusts for its baseline, and Hermes rounds the figure its own status line shows.
    pub fn prefers_reported_percentage(&self) -> bool {
        matches!(self, Self::Codex | Self::Hermes)
    }

    /// The catalog and record an icon uses: `contextDetailsAgentFor(icon) ?? 'claude'`.
    pub fn from_icon(icon: Option<&str>) -> Self {
        Self::for_icon(icon).unwrap_or(Self::Claude)
    }

    /// The other agent, which is where Copy to and Copy from write.
    pub fn other(&self) -> Self {
        match self {
            Self::Claude => Self::Codex,
            Self::Codex | Self::Cursor | Self::Hermes | Self::Pi | Self::Basic => Self::Claude,
        }
    }

    /// The name the Context details dialog uses.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Cursor => "Cursor",
            Self::Hermes => "Hermes",
            Self::Pi => "Pi",
            Self::Basic => "This agent",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeCost {
    pub total_usd: Option<f64>,
    pub duration_ms: Option<f64>,
    pub api_duration_ms: Option<f64>,
    pub lines_added: Option<f64>,
    pub lines_removed: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeRateLimitWindow {
    pub used_percentage: Option<f64>,
    /// Epoch seconds.
    pub resets_at: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeRateLimits {
    pub five_hour: Option<ClaudeRateLimitWindow>,
    pub seven_day: Option<ClaudeRateLimitWindow>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudePromptCache {
    pub warm: Option<bool>,
    /// Epoch seconds.
    pub expires_at: Option<f64>,
    pub hit_ratio: Option<f64>,
    pub requests: Option<f64>,
    pub misses: Option<f64>,
    pub last_miss_cause: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastRequestTokens {
    pub input_tokens: Option<f64>,
    pub output_tokens: Option<f64>,
    pub cache_read_tokens: Option<f64>,
    pub cache_write_tokens: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    pub host: Option<String>,
    pub owner: Option<String>,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestInfo {
    pub number: Option<f64>,
    pub url: Option<String>,
    pub review_state: Option<String>,
}

/// Codex's cumulative token counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexTokens {
    pub input_tokens: Option<f64>,
    pub cached_input_tokens: Option<f64>,
    pub cache_write_input_tokens: Option<f64>,
    pub output_tokens: Option<f64>,
    pub reasoning_output_tokens: Option<f64>,
    pub total_tokens: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexRateLimitWindow {
    pub used_percentage: Option<f64>,
    pub window_minutes: Option<f64>,
    /// Epoch seconds.
    pub resets_at: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexCredits {
    pub has_credits: Option<bool>,
    pub unlimited: Option<bool>,
    pub balance: Option<String>,
}

/// Codex-reported values from persisted rollout events.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexStatus {
    pub total_tokens: Option<CodexTokens>,
    pub turn_tokens: Option<CodexTokens>,
    pub last_request: Option<CodexTokens>,
    pub primary: Option<CodexRateLimitWindow>,
    pub secondary: Option<CodexRateLimitWindow>,
    pub credits: Option<CodexCredits>,
    pub plan: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub version: Option<String>,
    pub provider: Option<String>,
    pub current_dir: Option<String>,
    pub session_id: Option<String>,
    pub parent_thread_id: Option<String>,
    pub forked_from_id: Option<String>,
    pub started_at: Option<String>,
    pub approval_policy: Option<String>,
    pub sandbox: Option<String>,
    pub last_turn_duration_ms: Option<f64>,
    pub time_to_first_token_ms: Option<f64>,
}

/// `SessionChatCursorStatus`: Cursor's statusline payload and the session
/// checkout's git state (`server/src/session_chat_cursor_status.rs`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorStatus {
    pub version: Option<String>,
    pub current_dir: Option<String>,
    /// The checkout's folder name. Cursor's own `project_dir` is its internal
    /// `~/.cursor/projects/…` folder, so gxserver names the repository instead.
    pub repo: Option<String>,
    pub worktree: Option<String>,
    pub output_style: Option<String>,
    pub total_output_tokens: Option<f64>,
    pub autorun: Option<bool>,
    pub max_mode: Option<bool>,
    pub branch: Option<String>,
    pub lines_added: Option<f64>,
    pub lines_removed: Option<f64>,
    pub pr_number: Option<f64>,
    pub pr_state: Option<String>,
}

/// `hermesStatus`: the Hermes session's own row in its session store
/// (`server/src/session_chat_hermes_status.rs`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HermesStatus {
    pub model: Option<String>,
    /// Epoch seconds.
    pub started_at: Option<f64>,
    /// Epoch seconds, once the session ended.
    pub ended_at: Option<f64>,
    /// Input Hermes did not read from its cache.
    pub input_tokens: Option<f64>,
    pub cache_read_tokens: Option<f64>,
    pub cache_write_tokens: Option<f64>,
    pub output_tokens: Option<f64>,
    pub cost_usd: Option<f64>,
}

/// `piStatus`: what the Pi session's own transcript reports
/// (`server/src/session_chat_pi_status.rs`). Token and cost figures are the whole session's.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiStatus {
    /// `provider/id`.
    pub model: Option<String>,
    /// The model's name in Pi's lineup.
    pub model_name: Option<String>,
    pub thinking_level: Option<String>,
    /// Epoch milliseconds.
    pub started_at: Option<f64>,
    pub input_tokens: Option<f64>,
    pub output_tokens: Option<f64>,
    pub cache_read_tokens: Option<f64>,
    pub cache_write_tokens: Option<f64>,
    pub cost_usd: Option<f64>,
    pub context_tokens: Option<f64>,
    pub context_window: Option<f64>,
}

/// One usage window of a saved account.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountUsageWindow {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub used_percent: f64,
    pub limit_window_seconds: Option<f64>,
    /// An ISO stamp, unlike the agents' own epoch seconds.
    pub resets_at: Option<String>,
    pub model: Option<String>,
}

/// The slice of a saved account the context rows read.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAccount {
    #[serde(default)]
    pub id: String,
    /// `claude` or `codex`; a session only reads its own family's account.
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub usage: Vec<AccountUsageWindow>,
    pub reset_credits: Option<f64>,
    pub usage_updated_at: Option<String>,
    pub usage_error: Option<String>,
    #[serde(default)]
    pub session_count: f64,
}

/// What the whole row catalog reads.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextDetailStatus {
    // Claude's statusline payload, which Codex partly fills in too.
    pub cost: Option<ClaudeCost>,
    pub rate_limits: Option<ClaudeRateLimits>,
    pub prompt_cache: Option<ClaudePromptCache>,
    pub last_request: Option<LastRequestTokens>,
    pub total_output_tokens: Option<f64>,
    pub thinking_enabled: Option<bool>,
    pub output_style: Option<String>,
    pub version: Option<String>,
    pub repo: Option<RepoInfo>,
    pub project_dir: Option<String>,
    pub current_dir: Option<String>,
    pub pr: Option<PullRequestInfo>,
    // The chat's own additions.
    pub codex: Option<CodexStatus>,
    pub cursor: Option<CursorStatus>,
    pub hermes: Option<HermesStatus>,
    pub pi: Option<PiStatus>,
    pub account: Option<AgentAccount>,
    /// The context meter's used share (42%).
    pub context_used_percent: Option<String>,
    /// The context meter's token counts (84k/200k).
    pub context_tokens: Option<String>,
    pub model_name: Option<String>,
    pub effort_name: Option<String>,
    /// The checked-out branch, for the Basic catalog (Cursor's lives in `cursor`).
    pub branch: Option<String>,
}

/// One detected choice: the value the agent reported and its label.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedChoice {
    pub value: Option<String>,
    pub label: Option<String>,
}

/// `checkoutStatus`: the session checkout's git state, which gxserver sends for the agents that
/// report no status of their own (`server/src/session_chat_cursor_status.rs`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutStatus {
    pub repo: Option<String>,
    pub branch: Option<String>,
    pub current_dir: Option<String>,
}

/// The slice of `selectedOptions` the context surfaces read.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedOptions {
    pub model: Option<DetectedChoice>,
    pub effort: Option<DetectedChoice>,
    /// The complete normalized terminal line that supplied the detected values.
    pub terminal_status_line: Option<String>,
    pub context_usage: Option<ContextUsage>,
    /// The rest of Claude's statusline payload the chat can show.
    pub claude_status: Option<ClaudeStatus>,
    pub codex_status: Option<CodexStatus>,
    pub cursor_status: Option<CursorStatus>,
    pub hermes_status: Option<HermesStatus>,
    pub pi_status: Option<PiStatus>,
    pub checkout_status: Option<CheckoutStatus>,
}

/// Claude's own statusline payload, the part the rows read.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeStatus {
    pub cost: Option<ClaudeCost>,
    pub rate_limits: Option<ClaudeRateLimits>,
    pub prompt_cache: Option<ClaudePromptCache>,
    pub last_request: Option<LastRequestTokens>,
    pub total_output_tokens: Option<f64>,
    pub thinking_enabled: Option<bool>,
    pub output_style: Option<String>,
    pub version: Option<String>,
    pub repo: Option<RepoInfo>,
    pub project_dir: Option<String>,
    pub current_dir: Option<String>,
    pub pr: Option<PullRequestInfo>,
}

/// `cursorCommonStatus`: Cursor's values under the Claude field names the shared rows read.
fn cursor_common_status(cursor: &CursorStatus) -> ContextDetailStatus {
    ContextDetailStatus {
        version: cursor.version.clone(),
        current_dir: cursor.current_dir.clone(),
        output_style: cursor.output_style.clone(),
        total_output_tokens: cursor.total_output_tokens,
        repo: cursor.repo.clone().map(|name| RepoInfo {
            name: Some(name),
            ..RepoInfo::default()
        }),
        cost: (cursor.lines_added.is_some() || cursor.lines_removed.is_some()).then(|| {
            ClaudeCost {
                lines_added: cursor.lines_added,
                lines_removed: cursor.lines_removed,
                ..ClaudeCost::default()
            }
        }),
        pr: cursor.pr_number.map(|number| PullRequestInfo {
            number: Some(number),
            review_state: cursor.pr_state.clone(),
            ..PullRequestInfo::default()
        }),
        ..ContextDetailStatus::default()
    }
}

/// `resolveContextDetailStatus`.
pub fn resolve_context_detail_status(
    agent: ContextDetailsAgent,
    options: Option<&DetectedOptions>,
    account: Option<&AgentAccount>,
) -> ContextDetailStatus {
    let codex = match agent {
        ContextDetailsAgent::Codex => options.and_then(|options| options.codex_status.clone()),
        _ => None,
    };
    let hermes = match agent {
        ContextDetailsAgent::Hermes => options.and_then(|options| options.hermes_status.clone()),
        _ => None,
    };
    let pi = match agent {
        ContextDetailsAgent::Pi => options.and_then(|options| options.pi_status.clone()),
        _ => None,
    };
    let usage = resolve_context_meter_usage(
        options.and_then(|options| options.context_usage.as_ref()),
        agent.prefers_reported_percentage(),
    );
    let mut status = match agent {
        ContextDetailsAgent::Claude => {
            let claude = options.and_then(|options| options.claude_status.clone());
            match claude {
                Some(claude) => ContextDetailStatus {
                    cost: claude.cost,
                    rate_limits: claude.rate_limits,
                    prompt_cache: claude.prompt_cache,
                    last_request: claude.last_request,
                    total_output_tokens: claude.total_output_tokens,
                    thinking_enabled: claude.thinking_enabled,
                    output_style: claude.output_style,
                    version: claude.version,
                    repo: claude.repo,
                    project_dir: claude.project_dir,
                    current_dir: claude.current_dir,
                    pr: claude.pr,
                    ..ContextDetailStatus::default()
                },
                None => ContextDetailStatus::default(),
            }
        }
        ContextDetailsAgent::Cursor => options
            .and_then(|options| options.cursor_status.as_ref())
            .map(cursor_common_status)
            .unwrap_or_default(),
        ContextDetailsAgent::Basic => options
            .and_then(|options| options.checkout_status.as_ref())
            .map(|checkout| ContextDetailStatus {
                repo: checkout.repo.clone().map(|name| RepoInfo {
                    name: Some(name),
                    ..RepoInfo::default()
                }),
                current_dir: checkout.current_dir.clone(),
                branch: checkout.branch.clone(),
                ..ContextDetailStatus::default()
            })
            .unwrap_or_default(),
        ContextDetailsAgent::Pi => {
            let checkout = options.and_then(|options| options.checkout_status.as_ref());
            ContextDetailStatus {
                cost: pi
                    .as_ref()
                    .and_then(|pi| pi.cost_usd)
                    .map(|total_usd| ClaudeCost {
                        total_usd: Some(total_usd),
                        ..ClaudeCost::default()
                    }),
                total_output_tokens: pi.as_ref().and_then(|pi| pi.output_tokens),
                repo: checkout
                    .and_then(|checkout| checkout.repo.clone())
                    .map(|name| RepoInfo {
                        name: Some(name),
                        ..RepoInfo::default()
                    }),
                current_dir: checkout.and_then(|checkout| checkout.current_dir.clone()),
                branch: checkout.and_then(|checkout| checkout.branch.clone()),
                ..ContextDetailStatus::default()
            }
        }
        ContextDetailsAgent::Hermes => ContextDetailStatus {
            cost: hermes
                .as_ref()
                .and_then(|hermes| hermes.cost_usd)
                .map(|total_usd| ClaudeCost {
                    total_usd: Some(total_usd),
                    ..ClaudeCost::default()
                }),
            ..ContextDetailStatus::default()
        },
        ContextDetailsAgent::Codex => {
            let request = codex.as_ref().and_then(|codex| codex.last_request);
            ContextDetailStatus {
                version: codex.as_ref().and_then(|codex| codex.version.clone()),
                current_dir: codex.as_ref().and_then(|codex| codex.current_dir.clone()),
                total_output_tokens: codex
                    .as_ref()
                    .and_then(|codex| codex.total_tokens)
                    .and_then(|tokens| tokens.output_tokens),
                last_request: request.map(|request| LastRequestTokens {
                    input_tokens: request
                        .input_tokens
                        .map(|input| (input - request.cached_input_tokens.unwrap_or(0.0)).max(0.0)),
                    output_tokens: request.output_tokens,
                    cache_read_tokens: request.cached_input_tokens,
                    cache_write_tokens: request.cache_write_input_tokens,
                }),
                ..ContextDetailStatus::default()
            }
        }
    };
    status.codex = codex.clone();
    status.cursor = match agent {
        ContextDetailsAgent::Cursor => options.and_then(|options| options.cursor_status.clone()),
        _ => None,
    };
    status.hermes = hermes;
    status.pi = pi;
    status.account = account
        .filter(|account| account.provider == agent.as_str())
        .cloned();
    status.model_name = options
        .and_then(|options| options.model.as_ref())
        .and_then(|model| model.label.clone())
        .or_else(|| codex.as_ref().and_then(|codex| codex.model.clone()))
        .or_else(|| {
            status
                .hermes
                .as_ref()
                .and_then(|hermes| hermes.model.clone())
        })
        .or_else(|| {
            let pi = status.pi.as_ref()?;
            pi.model_name.clone().or_else(|| pi.model.clone())
        });
    status.effort_name = options
        .and_then(|options| options.effort.as_ref())
        .and_then(|effort| effort.label.clone())
        .or_else(|| codex.as_ref().and_then(|codex| codex.effort.clone()))
        .or_else(|| status.pi.as_ref().and_then(|pi| pi.thinking_level.clone()));
    status.context_used_percent =
        usage.and_then(|usage| format_context_percentage(usage.used_percentage));
    status.context_tokens = usage.and_then(|usage| {
        usage.used_tokens.map(|used| {
            format!(
                "{}{}",
                format_context_tokens(Some(used)),
                match usage.window_size {
                    None => String::new(),
                    Some(window) => format!("/{}", format_context_tokens(Some(window))),
                }
            )
        })
    });
    status
}
