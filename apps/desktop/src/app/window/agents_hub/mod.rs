//! Native GPUI Agents Hub, the desktop twin of the React `AgentsHubModal` in
//! packages/core-ui/agents-hub-modal.tsx (deleted 2026-10-01) and its Agent Sync tab in packages/core-ui/agents-hub-sync/.
//!
//! CDXC:AgentLauncher 2026-09-28 DECISION:
//! User: migrate every React modal to GPUI with GPUI-Kit components, looking and working exactly as now, so the app runs without CEF. The Hub keeps its five tabs (Skills, MDs, Hooks, Configs & MCPs on Cmd/Ctrl+1..4, Agent Sync on Cmd/Ctrl+5), the searchable grouped file list, the file editor with Copy path, Open containing folder, Open in built-in editor, Refresh and Save, and Agent Sync's agent list, overview, per-agent cards and plan sheet, in both appearances and under window glass. It still does not close when clicked away (CDXC:AppModal 2026-09-27).
//!
//! CDXC:AgentLauncher 2026-09-28 WHY: Cmd/Ctrl+S saves the open file. Monaco in the React Hub had no save binding; the native editor adds the one every editor has, since it replaces Monaco anyway.
//! SEE-ALSO: packages/core-ui/styles/agents-hub.css (deleted 2026-11-06), styles/modals-light.css (deleted 2026-11-06) and styles/modals-glass.css (deleted 2026-11-06) (the React tokens mirrored in palette.rs), apps/desktop/src/app/agents_hub_modal_lifecycle.rs (open, commands, answers), apps/desktop/src/app/helpers/agents_hub/ (catalog, file reads and writes, Agent Sync), apps/desktop/src/bin/native_modal_demo/agents_hub.rs (standalone preview).
//!
//! The module depends only on gpui, gpui-component, serde and the modal kit, so the preview
//! binary can include it with `#[path]`.
mod editor;
mod files_tab;
mod model;
mod palette;
mod plan_sheet;
mod sync_agent;
mod sync_model;
mod sync_overview;
mod sync_tab;
mod widgets;
mod window;

pub(crate) use model::{AgentsHubCatalog, AgentsHubFileContent, AgentsHubTab};
pub(crate) use sync_model::{SyncApplyResult, SyncPlan, SyncReport};
pub(crate) use window::{
    AGENTS_HUB_MODAL_HEIGHT, AGENTS_HUB_MODAL_WIDTH, AgentsHubModalCommand, AgentsHubModalConfig,
    AgentsHubModalHost, GpuiAgentsHubModalWindow,
};
