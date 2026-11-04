use super::*;

// Master constant table (upstream chat spec §7.1).
pub const SESSION_CHAT_SUBMIT_DELAY_MS: u64 = 500;
pub const SESSION_CHAT_QUESTION_STEP_MS: u64 = 1_000;
pub const SESSION_CHAT_IMAGE_ATTACHMENT_SETTLE_MS: u64 = 300;
/*
The clear burst must reach the TUI in its OWN stdin read chunk. Written
back-to-back with a paste frame, the two coalesce into one chunk, and Claude
Code's chunk-level paste handling inserts the burst bytes as literal text at
the head of the message instead of interpreting them as kill keys (observed
2026-08-23: a chat-sent prompt was recorded with 39×Ctrl-U + 39×Ctrl-K glued
to its front, which also left the optimistic echo unconsumed — the duplicated
user bubble). Same pacing discipline as the image settle and the separate
delayed Enter.
*/
pub const SESSION_CHAT_CLEAR_INPUT_SETTLE_MS: u64 = 150;
pub const SESSION_CHAT_SUBMIT: &str = "\r";
/// Empryo's Alt+Q, which sends its input at once while idle and queues it as a turn of its own
/// while one runs; Enter there would steer the running turn instead.
pub const SESSION_CHAT_EMPRYO_SUBMIT: &str = "\u{1b}q";
/// Claude Code's `chat:sendNow` chord, Ctrl+X Ctrl+S: it stops the running turn and sends what
/// Claude has queued, and does nothing while no turn runs.
pub const SESSION_CHAT_CLAUDE_SEND_NOW: &str = "\u{18}\u{13}";
/*
CDXC:Clipboard 2026-08-24:
Why the Enter is closed-loop. The old sequence wrote the paste body, slept
SESSION_CHAT_SUBMIT_DELAY_MS, then wrote a bare "\r" with nothing checking
that the composer had taken the body. Claude Code ingests a multi-KB paste
asynchronously — it paints "Pasting text…" and only later collapses the body
into a "[Pasted text #N +M lines]" placeholder — and under machine load that
ingestion takes LONGER than 500ms. Reproduced deterministically 2026-08-23
with a 69-line / 4.6KB message: the bare Enter submitted an EMPTY composer,
the body arrived afterwards, and the user's message was silently lost because
it remained stranded in the terminal composer. zmx transport was byte-perfect
at that size, so the race is purely TUI ingestion time versus a fixed delay.

The fix is to watch the screen instead of the clock: settle briefly (so a
paste that already landed costs no extra latency), then poll captures until
the body is provably on screen. A send whose body cannot be proven present is
ABORTED without an Enter — losing the send with an error the user sees is
strictly better than submitting an empty turn and dropping their text.
*/
pub const SESSION_CHAT_VERIFY_SETTLE_MS: u64 = SESSION_CHAT_SUBMIT_DELAY_MS;
pub const SESSION_CHAT_VERIFY_POLL_MS: u64 = 150;
pub const SESSION_CHAT_VERIFY_MIN_TIMEOUT_MS: u64 = 2_000;
pub const SESSION_CHAT_VERIFY_MAX_TIMEOUT_MS: u64 = if cfg!(windows) { 30_000 } else { 8_000 };

/*
CDXC:SessionChat 2026-08-26:
The window `SessionChatSendStep::WaitForComposer` gives a CLI to paint its input
box. Short settle because the overwhelming majority of sends go to a CLI that
has been idle for minutes and answers on the first capture; six seconds of
ceiling because the slow case is a cold agent process still loading its config,
skills and MCP servers, which was measured past four on this machine — the very
reason the old blind four-second sleeps kept losing first prompts.
*/
pub const SESSION_CHAT_COMPOSER_WAIT_SETTLE_MS: u64 = 0;
pub const SESSION_CHAT_COMPOSER_WAIT_TIMEOUT_MS: u64 = 6_000;
/// Bytes of payload per millisecond of extra patience: a paste twice the size
/// takes about twice as long to ingest.
pub const SESSION_CHAT_VERIFY_BYTES_PER_MS: u64 = 2;
/// "Pasting text…" is the TUI telling us ingestion is still running, so the
/// deadline is worth extending — once, so a wedged indicator cannot stall the
/// queue indefinitely.
pub const SESSION_CHAT_VERIFY_PASTING_EXTENSION_MS: u64 = 4_000;
/// The TUIs' collapsed large-paste placeholders. A body the composer collapsed
/// never shows its own text, so the placeholder is the only proof of landing.
/// Measured live 2026-08-24 with a 1KB / 28-line paste into every supported
/// agent: Claude Code `[Pasted text #1 +69 lines]`, Codex
/// `[Pasted Content 1037 chars]`, pi `[paste #1 +28 lines]`, Grok Build
/// `[Pasted: 28 lines]`, omp `[Paste #1, +28 lines]`. Five agents, five
/// spellings, one shared prefix — so the match is that prefix,
/// case-insensitively, against the whitespace-stripped screen. (Matching only
/// the Claude form aborted Codex sends that had in fact been delivered.)
/// OMP 18.11 draws its placeholder as a chip instead (`omp_paste_chip_shown`).
pub(super) const SESSION_CHAT_PASTED_PLACEHOLDER_NEEDLE: &str = "[paste";
/// Claude Code's still-ingesting indicator, `Pasting text…`, normalized.
pub(super) const SESSION_CHAT_PASTING_INDICATOR_NEEDLE: &str = "Pastingtext";
/// Shown to the user when the composer never took the body. Deliberately
/// describes the terminal, not the network: nothing was submitted.
pub const SESSION_CHAT_PASTE_NOT_ACCEPTED: &str = "The terminal did not accept the pasted message.";
/// Fallback for a composer wait that timed out without a per-agent reason.
pub const SESSION_CHAT_COMPOSER_NOT_READY: &str =
    "The agent's input box is not on screen, so nothing was sent.";
/// Empryo's input box shows text this terminal's screen capture cannot tell apart from its tip
/// (the capture has no cursor), so placing a Chat draft there, which would clear it, stops.
pub(super) const EMPRYO_DRAFT_UNREADABLE: &str =
    "This terminal's screen does not show its cursor, so Ghostex cannot tell text typed in Empryo's input box from its tip and did not replace it. The Chat draft is still in the chat.";
pub(super) const SESSION_CHAT_CLAUDE_PANEL_NOT_DISMISSED: &str =
    "The Claude Code panel over the input box did not close, so nothing was sent.";
pub(super) const SESSION_CHAT_CODEX_SIDE_NOT_CLOSED: &str =
    "Codex's side conversation did not close, so nothing was sent.";
/// What closes Codex's side conversation (session_chat_codex_side.rs).
pub(super) const SESSION_CHAT_CODEX_CLOSE_SIDE: &str = "\u{3}";
/// Escape presses one send spends on a panel that stays up.
pub(super) const SESSION_CHAT_CLAUDE_PANEL_ESCAPES: u64 = 3;
/// CDXC:SessionChat 2026-09-23 WHY:
/// Native Windows Codex reads console input records through ConPTY, which turns a bare Escape into VK_ESCAPE but does not decode CSI-u Escape. Chat Stop's CSI-u write left a live turn streaming until physical Escape interrupted it. POSIX zmx (including WSL) retains CSI-u: bare Escape was dropped by kitty-enabled Claude Code in the verified 2026-08-01 flow.
#[cfg(windows)]
pub const SESSION_CHAT_INTERRUPT: &str = "\u{1b}";
#[cfg(not(windows))]
pub const SESSION_CHAT_INTERRUPT: &str = "\u{1b}[27u";
/*
Shift+Tab in the kitty CSI-u encoding (CSI 9 ; 2 u — Tab with the Shift
modifier). Claude Code cycles its permission mode on it and has no
slash-command equivalent, so the chat surface injects the raw bytes.
Verified live 2026-08-01 against Claude Code v2.1.220 on a zmx pty: the legacy
back-tab "\x1b[Z" did nothing, while "\x1b[9;2u" cycled the footer through
auto → manual → accept edits → plan → bypass on every write. Same kitty-active
reasoning as SESSION_CHAT_INTERRUPT above.
*/
pub const SESSION_CHAT_SHIFT_TAB: &str = "\u{1b}[9;2u";
pub const SESSION_CHAT_SHIFT_UP: &str = "\u{1b}[1;2A";
pub const SESSION_CHAT_SHIFT_DOWN: &str = "\u{1b}[1;2B";
/*
The two kill keys, and deliberately no Ctrl+Y beside them. A yank returns only
the LAST kill, which after a 2N-1 burst is a fragment of a multi-line draft or
nothing at all, so no writer of this input line can restore what it cleared.
Writers discard (the chat-send policy); terminal→chat view switching stays the
loss-safe transfer.
*/
pub const AGENT_TUI_CLEAR_INPUT_LINE: &str = "\u{15}"; // Ctrl+U — clear toward start
pub const AGENT_TUI_CLEAR_INPUT_FORWARD: &str = "\u{b}"; // Ctrl+K — clear toward end
pub const AGENT_TUI_CLEAR_LINE_SLACK: usize = 8;
pub const AGENT_TUI_CLEAR_MAX_LINES: usize = 40;
pub(super) const SESSION_CHAT_DRAFT_PRESERVE_TIMEOUT: Duration = Duration::from_secs(16);
pub(super) const SESSION_CHAT_PROMPT_EDITOR_INPUT: &str = "\u{7}";
pub(super) const SESSION_CHAT_GROK_PROMPT_EDITOR_INPUT: &str = "\u{10}";
pub(super) const PROMPT_STASH_REQUEST_FRESHNESS: Duration = Duration::from_secs(15);
/// Longest single-line message written without a bracketed-paste frame, kept well under
/// one terminal read (1022 bytes on macOS) so an unframed send always arrives whole.
pub(super) const SESSION_CHAT_UNFRAMED_SINGLE_LINE_MAX_BYTES: usize = 512;
pub(super) const BRACKETED_PASTE_START: &str = "\u{1b}[200~";
pub(super) const BRACKETED_PASTE_END: &str = "\u{1b}[201~";
pub(super) static SESSION_CHAT_DRAFT_PRESERVE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// The refusal code of an answer the chat could not deliver, which the user can still give in the
/// agent's terminal.
///
/// CDXC:SessionChat 2026-10-29 WHY: GitHub #350: the refusal said "Answer it in the terminal" and offered no way there, in a chat the user may be reading without the terminal open. The chat (desktop and phone) shows an Open terminal button beside any refusal with this code.
/// SEE-ALSO: apps/desktop/src/app/native_chat/composer.rs, apps/mobile/app/src/chat/native/cards/ComposerNotReadyCard.tsx.
pub const ANSWER_IN_TERMINAL: &str = "answerInTerminal";
