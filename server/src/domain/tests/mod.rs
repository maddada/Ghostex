//! domain repository unit tests, grouped by what they cover. Shared fixtures (test
//! database, git and JSON field helpers) live in `fixtures.rs`.
mod fixtures;
mod project_paths;
mod project_rows_and_json;
mod project_updates;
mod session_rows;
mod stashed_prompts;

use fixtures::*;

use super::*;
