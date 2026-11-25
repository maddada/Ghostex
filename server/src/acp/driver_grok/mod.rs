//! Native `grok agent stdio` ACP + xAI extension driver for `ghostex acp-host --agent grok`.
//!
//! Speaks Grok CLI's stdio ACP JSON-RPC protocol with xAI's extension notifications and reverse
//! requests directly in Rust:
//! - Enforces Ghostex's four live approval levels (`Ask`, `Auto-edits`, `Auto`, `Full permissions`)
//!   matching Grok's `--permission-mode default`, `--permission-mode auto`, and `--always-approve`
//!   launch modes, with clean subprocess recycle when switching into/out of `Auto` or `--always-approve`.
//! - Sends `initialize` with `_meta: { clientType: "extension" }` and authenticates via
//!   `xai.api_key` (when `XAI_API_KEY` is set) or `cached_token`.
//! - Handles `session/load` replay-idle readiness (2s idle gap when Grok leaves `session/load`
//!   unanswered after replay finishes) and falls back to `session/resume`.
//! - Races `session/prompt` RPC responses against `x.ai/session/prompt_complete`,
//!   `_x.ai/session/prompt_complete`, and `turn_completed` session notifications.
//! - Supports `_x.ai/interject` mid-turn steering (with optional images), `session/cancel` with
//!   `_meta: { cancelTrigger: "ctrl_c" }` and process-group restart on stuck cancellation,
//!   `x.ai/ask_user_question` / `_x.ai/ask_user_question` interactive questions,
//!   `x.ai/exit_plan_mode` / `_x.ai/exit_plan_mode` plan gates, protocol v1/v2 model + reasoning
//!   effort selection, image prompt blocks, leading `$skill` -> `/skill` expansion via
//!   `grok inspect --json`, MCP `use_tool` normalization, background subagent / task / monitor
//!   tracking (`agentFleet`), wake continuation turns, context usage, `gxserver` reload survival,
//!   and Switch to Terminal (`grok -r <sessionId>`).

use std::time::Duration;

mod approvals;
mod events;
mod models;
mod process;
mod protocol;
mod run;
mod state;
mod transport;

pub(in crate::acp) use run::run;

pub(super) const GROK_RPC_TIMEOUT: Duration = Duration::from_secs(30);
pub(super) const GROK_INITIALIZE_TIMEOUT: Duration = Duration::from_secs(30);
pub(super) const GROK_LOAD_TIMEOUT: Duration = Duration::from_secs(90);
pub(super) const GROK_LOAD_REPLAY_IDLE_GAP: Duration = Duration::from_secs(2);
pub(super) const GROK_SKILLS_PROBE_TIMEOUT: Duration = Duration::from_secs(4);
pub(super) const GROK_SKILLS_CACHE_TTL: Duration = Duration::from_secs(30);
pub(super) const GROK_COMPLETED_PROMPT_ID_LIMIT: usize = 128;

pub(super) const GROK_API_KEY_ENV: &str = "XAI_API_KEY";
pub(super) const GROK_OAUTH2_REFERRER_ENV: &str = "GROK_OAUTH2_REFERRER";
pub(super) const GROK_OAUTH2_REFERRER_VALUE: &str = "ghostex";
pub(super) const GROK_AUTH_METHOD_API_KEY: &str = "xai.api_key";
pub(super) const GROK_AUTH_METHOD_CACHED_TOKEN: &str = "cached_token";
pub(super) const GROK_DEFAULT_MODEL_SLUG: &str = "grok-build";
pub(super) const GROK_ALLOW_EDITS_SESSION_OPTION_ID: &str = "allow-edits-session";
pub(super) const XAI_RATE_LIMITED_ERROR_CODE: i64 = -32003;
pub(super) const XAI_TASK_COMPLETED_PROMPT_ID_PREFIX: &str = "task-completed-";
pub(super) const XAI_SUBAGENT_COMPLETED_PROMPT_ID_PREFIX: &str = "subagent-completed-";
pub(super) const XAI_INTERJECT_METHOD: &str = "_x.ai/interject";
