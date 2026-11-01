//! Characterization tests: what Issues Workflow publishes for Linear and GitHub today (the Work
//! list, a session's work fields and links, branch matching, item refs and the tracker default),
//! from recorded tracker answers seeded into the caches, never the network. They assert the JSON
//! clients read, so a change to any Linear or GitHub field fails here.
mod background;
mod command_contract;
mod command_icon;
mod command_limits;
mod command_masking;
#[cfg(unix)]
mod command_runner;
mod fixtures;
mod identity;
mod link_add;
mod matching;
mod pull_request_links;
mod session_work;
mod tracker_default;
mod work_list;

use fixtures::*;

use crate::session_git_status::PullRequestState;

use super::*;
