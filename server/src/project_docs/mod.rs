//! Project Docs actions (the Docs surface's file tree, previews, file operations and git
//! baselines), split by concern from the former `project_docs.rs`. Every submodule is glob
//! re-exported here, so callers keep using `project_docs::…` unchanged.

mod actions;
mod chat_file;
mod file_ops;
mod git_baseline;
mod limits;
mod listing;
mod project_scope;
mod roots;
mod scan_roots;
mod tree;

use actions::*;
pub use chat_file::*;
use file_ops::*;
use git_baseline::*;
pub use limits::*;
pub use roots::*;
use scan_roots::*;
use tree::*;

// `listing` and `project_scope` read these through `use super::*`.
use serde_json::Map;
use std::path::Path;
