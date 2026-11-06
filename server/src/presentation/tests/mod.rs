//! presentation unit tests, grouped by what they cover. Shared fixtures (project, session
//! and previous-session rows, test database) live in `fixtures.rs`.
mod attention;
mod fixtures;
mod issues_workflow;
mod search_and_previous_sessions;
mod sidebar_snapshot;
mod title_projection;

use fixtures::*;

use super::*;
