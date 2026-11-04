//! In-app feedback: the issue draft a client shows the user for review, the send that posts
//! the reviewed issue to the Ghostex feedback relay, which files it as a GitHub issue, and the
//! feedback agent that gathers details before it files one.

mod agent;
mod brief_wait;
mod draft;
mod http;
mod relay;

pub(crate) use agent::handle_start_feedback_agent_http;
pub(crate) use http::handle_feedback_http;
