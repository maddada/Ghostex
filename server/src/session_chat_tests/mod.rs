//! Session Chat transcript tests, split by concern (decoders, prompt state, successor
//! adoption, readers, real-transcript checks). Test-only module.

use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::resume_lookup::home_dir;
use crate::session_chat::*;

mod claude_compact_offer;
mod claude_plan_dialog;
mod decoders;
mod prompt_state;
mod readers;
mod real_transcripts;
mod successor;

use readers::*;
