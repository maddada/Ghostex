//! Creating things from the sidebar and its hosts: which project a create names, and what a
//! browser open, a terminal create or an agent launch does, as data the host performs.
//!
//! Per-concern files: `target` resolves the sidebar group id a create names into a project on this
//! computer or on a remote machine, exactly as the old runtime's `session-create.ts` did,
//! `browser` plans the two browser opens (a project header's New Browser Tab and the Quick
//! Browser Tab), `agents` reads the launcher agent a launch names off the HUD and derives its hook
//! family, default title and first-prompt title settings, and `params` builds every create's
//! gxserver parameters; `board_links` and `board_state` are the Project Board's conversation links and
//! what its cards show about them.

mod agents;
mod board_links;
mod board_state;
mod browser;
mod params;
mod target;

pub use agents::{
    agent_session_default_title, default_agent_id_for_icon, first_prompt_title_runtime_settings,
    resolve_sidebar_agent, SidebarAgent, TitleGenerationSettings,
};
pub use board_links::{
    bead_conversation_link_id, bead_conversation_link_match_key, board_session_id,
    canonicalize_links_for_board, normalize_bead_conversation_links, project_board_links,
    resolve_board_session_id, select_link_store_projects, BeadConversationLink,
};
pub use board_state::{
    board_agent_options, board_reference, link_view, BoardSession, BoardSessionFacts,
    BoardSessionOption, LinkAvailability,
};
pub use browser::{plan_browser_pane_open, BrowserPaneOpen, DEFAULT_BROWSER_LAUNCH_URL};
pub use params::{
    agent_record_params, check_startup_prompt_receipt, created_session, local_agent_launch_params,
    os_integration_command_params, queue_startup_prompt_params, remote_agent_launch_params,
    start_provider_params, terminal_create_params, AgentRecordOptions,
};
pub use target::{
    group_project, normalize_project_path, project_name_from_path, terminal_create_target,
    CreateTarget,
};
