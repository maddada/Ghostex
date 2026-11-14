//! The Context group of Claude Code's row panel in Settings > Agents: Compact idle chats before the
//! prompt cache expires, Only above (shown while that switch is on) and Use a 1-hour prompt cache.
//! gxserver hands them to each Claude launch and resume (server/src/agents/claude_idle_compaction.rs,
//! where the user's decision is).
//!
//! CDXC:AgentProviders 2026-11-14 SEE-ALSO: the search rows in packages/settings-catalog/src/pages/agents.rs, which open this panel when a search finds one of them (`search_opens_claude`).

use super::super::super::catalog::{module, settings_catalog};
use super::super::super::fields::{RowSpec, reset_key, select_field, toggle_field};
use super::super::super::palette::SettingsPalette;
use super::super::super::search::TabSearch;
use super::AgentsTab;
use gpui::{AnyElement, Context, Window};
use serde_json::json;

const IDLE_COMPACTION: &str = "claudeIdleCompaction";
const MIN_TOKENS: &str = "claudeIdleCompactAbove";
const ONE_HOUR_CACHE: &str = "claudeOneHourPromptCache";

/// The agent whose panel holds the group.
pub(super) const CLAUDE_AGENT_ID: &str = "claude";

/// Whether a search found one of the group's rows, which opens Claude Code's panel to show it.
pub(super) fn search_opens_claude(search: &TabSearch) -> bool {
    let section = search.section("agentList");
    section.is_searching
        && [IDLE_COMPACTION, MIN_TOKENS, ONE_HOUR_CACHE]
            .iter()
            .any(|key| section.visible_keys.contains(*key))
}

impl AgentsTab {
    /// The group's rows, each in the panel's own row shape (`panel_child`).
    pub(super) fn claude_context_rows(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let values = self.store.read(cx).values();
        let idle_compaction = values.bool(IDLE_COMPACTION);
        let mut rows = vec![toggle_field(
            self,
            p,
            IDLE_COMPACTION,
            RowSpec::new("Compact idle chats before the prompt cache expires")
                .description(
                    "Claude Code compacts a long chat that has been idle for about 54 minutes, just before its 1-hour prompt cache expires, so your next message doesn't re-read the whole chat uncached. Runs only on a 1-hour cache. Applies to Claude sessions started or resumed after you change it.",
                )
                .keyed(&values, IDLE_COMPACTION),
            idle_compaction,
            cx,
        )];
        if idle_compaction {
            let options = settings_catalog()
                .options(module::SETTINGS, "CLAUDE_IDLE_COMPACT_MIN_TOKENS_OPTIONS");
            let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
            let min_tokens = values.choice(MIN_TOKENS, &allowed);
            rows.push(select_field(
                self,
                p,
                MIN_TOKENS,
                RowSpec::new("Only above")
                    .description(
                        "Compact only chats whose context is at least this size. Claude Code's default is 200K.",
                    )
                    .dependent()
                    .keyed(&values, MIN_TOKENS),
                Some(reset_key::<Self>(MIN_TOKENS)),
                &options,
                &min_tokens,
                None,
                |page: &mut Self, value, _window, cx| page.save(MIN_TOKENS, json!(value), cx),
                window,
                cx,
            ));
        }
        rows.push(toggle_field(
            self,
            p,
            ONE_HOUR_CACHE,
            RowSpec::new("Use a 1-hour prompt cache")
                .description(
                    "Ask for Claude's 1-hour prompt cache on an API key, Bedrock, Vertex or Foundry, which otherwise get 5 minutes, so idle compaction runs there too. 1-hour cache writes cost more than 5-minute ones. A subscription already gets 1 hour.",
                )
                .keyed(&values, ONE_HOUR_CACHE),
            values.bool(ONE_HOUR_CACHE),
            cx,
        ));
        rows
    }
}
