//! Session Chat terminal activity detection, split by concern. Every submodule is glob
//! re-exported here, so `crate::session_chat_terminal_activity::*` paths are unchanged.

/*
CDXC:AgentScreenDetection 2026-08-22:
Live work the agent CLI reports ONLY on its terminal screen, before the same
text reaches the transcript. Claude Code replaces a current-status line as it
works:

    ⏺ Removing temporary examples

It also reports a small set of meaningful states under other markers:

    ✻ Waiting for 1 dynamic workflow to finish
    ✻ Cooked for 46s · 1 shell still running

The client keeps each general status change as a transient reasoning row, then
lets the authoritative transcript replace it when JSONL catches up. A running
shell stays as one bottom activity row only while it remains on screen.
Claude's compaction is the structured-progress variant:

    ❯ /compact

    ✶ Compacting conversation… (1m 1s)
      ████████████████████░░░░░░░░░░░░░░░░░░░░ 49%
    Tip: Use /btw to ask a quick side question without interrupting Claude's…

For a minute or more the chat surface could say nothing better than "the agent
is working", because a transcript projection cannot see a progress bar. Worse,
compaction is the one operation whose whole point is that the conversation the
user is reading is about to be REPLACED — so a bare typing indicator is not
just uninformative, it hides the single most consequential thing happening.

This is deliberately NOT a terminal notice: nothing is wrong, nothing is
blocked, and there is nothing to answer. Both variants render in the transcript
where the work is.

Parsing is narrow and evidence-only. `⏺` owns Claude's general status rows;
other star markers are accepted only for explicitly understood states. The
percentage and elapsed clock are read off the screen or omitted; neither is
ever estimated.
*/

use crate::session_chat_options::{session_chat_option_agent, SessionChatOptionAgent};

use serde_json::{json, Map, Value};

mod agent_stream;
mod line_parsers;
mod screen_rows;
mod shell_command;
mod types;

pub use agent_stream::*;
pub(crate) use line_parsers::*;
pub use screen_rows::*;
pub use shell_command::*;
pub use types::*;
