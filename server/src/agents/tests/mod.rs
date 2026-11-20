//! agents module unit tests, grouped by what they cover. Shared fixtures (test database,
//! metadata and agent session builders) live in `fixtures.rs`.
mod activity_and_ownership;
mod first_prompt;
mod fixtures;
mod hook_and_state_events;
mod lead_links;
mod live_process_identity;
mod metadata_title_reconcile;
mod settings_and_launch_plans;
mod title_capture;

use fixtures::*;

use super::*;
