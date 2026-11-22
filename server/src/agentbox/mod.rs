//! AgentBox (Cloud Boxes): agent sessions that run inside an agentbox sandbox, on this computer's
//! Docker or in the cloud, driven through the `agentbox` CLI. Split by concern; see each file.

mod activity;
mod cli;
mod command;
mod create;
pub(crate) mod endpoint;
mod first_prompt;
mod input_ready;
mod lifecycle;
mod location;
mod restore;
mod session;
mod status;

pub(crate) use activity::{start_agentbox_activity_poller, wake_agentbox_activity_poller};
pub(crate) use create::{box_agent_family, prepare_box_launch};
pub(crate) use first_prompt::claim_launch_prompt_echo;
pub(crate) use input_ready::{is_agentbox_session_by_ids, wait_for_box_agent_input};
pub(crate) use lifecycle::stop_session_box_in_background;
pub(crate) use location::{
    provider_label, requested_agentbox_provider, run_location_from_cli, AGENTBOX_AGENTS,
};
pub(crate) use restore::box_resume_plan;
pub(crate) use session::{
    is_agentbox_session, pending_session_agentbox, presentation_agentbox_value,
    refuse_for_agentbox_session, session_agentbox, without_client_agentbox_record, PENDING_KEY,
};
pub(crate) use status::{claude_signed_in_for_boxes, read_status};
