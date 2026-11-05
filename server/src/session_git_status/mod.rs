//! Sidebar V2 git/PR status, split by concern. Every submodule is glob re-exported here,
//! so `crate::session_git_status::*` paths are unchanged.

use std::{
    collections::{HashMap, HashSet},
    io::Read,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

use serde_json::{json, Map, Value};

/*
CDXC:Git 2026-07-29-00:00:
Server side of Sidebar V2's git/PR card row (spec `plans/009-sidebar-v2-inbox.md`,
decision 7 + the P3 wire contract). Each worktree session's cwd IS the checkout it
works in, so per-session git state is resolved from the session cwd and published
on `GxserverPresentationSession.gitStatus`:

  { branch: string|null, additions: number, deletions: number,
    prNumber?: number, prState?: "open"|"draft"|"merged"|"closed",
    prUrl?: string, updatedAt: string }

Rules this module holds itself to:

- One probe per UNIQUE cwd. Many sessions share a checkout (a project's terminal,
  its agent, its browser row), so the cache is keyed by cwd and every session
  pointing at it reads the same answer.
- Never on a request path. A background pass (server.rs) refreshes the cache;
  presentation and the lifecycle sweep only ever READ it, and read a cwd that has
  never been probed as "no git status" rather than probing inline.
- Every subprocess is time-boxed and killed on timeout, so a hung git (network
  filesystem, credential prompt) can never wedge the daemon.
- `gh` absent or unauthed is not an error: the PR fields simply do not exist. The
  detection itself is cached so a machine without `gh` spawns one probe per
  `GH_AVAILABILITY_TTL`, not one per pass.
- Non-git cwds are cached as negative entries (a longer TTL than real repos —
  a directory rarely becomes a repository) so a terminal parked in ~/Downloads
  does not cost a git spawn a minute.
*/

mod commands;
mod git_plumbing;
mod model;
mod pull_request_batch;
mod refresh;
#[cfg(test)]
mod tests;

pub use commands::*;
pub use git_plumbing::*;
pub use model::*;
pub use refresh::*;
