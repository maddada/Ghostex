//! In-app feedback: the issue draft a client shows the user for review, the send that posts
//! the reviewed issue to the Ghostex feedback relay, which files it as a GitHub issue, and the
//! feedback agent that gathers details before it files one, and the steps any agent (that one, or
//! Ask Ghostex AI) follows to file one.

mod agent;
mod brief;
mod brief_wait;
mod draft;
mod http;
mod relay;

pub(crate) use agent::handle_start_feedback_agent_http;
pub(crate) use brief::{FEEDBACK_BRIEF_ENDPOINT, handle_feedback_brief_http};
pub(crate) use http::handle_feedback_http;
