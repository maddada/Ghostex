/*
CDXC:Terminal 2026-07-03:
P1b portable terminal model: PTY (portable-pty: openpty on Unix, ConPTY on
Windows) → libghostty-vt → owned per-frame snapshots. Pure model layer with
no gpui types; consumers (P1c element, P1e integration) observe it through
TerminalEventSink and pull TerminalSnapshot values.

Threading (four plain std threads per model; pty-read/wakeup/child-wait exit
when the child dies, pty-write exits after viewer retirement or once its
channel senders are gone):
- pty-read: blocking PTY reads, feeds bytes into the shared VtTerminal under
  a SHORT lock (feed only), then requests a wakeup.
- pty-write: owns the PTY write half and drains a channel of byte payloads
  (main-thread input and in-feed VT auto-replies), so a stalled PTY
  (suspended child, XOFF'd tty, full kernel buffer) can never block the
  main thread in write_all.
- wakeup: coalesces wakeup requests. First bytes after a delivered wakeup arm
  a ~4ms window; every burst inside the window folds into ONE Wakeup event
  (idea from Zed's terminal wakeup batching; implementation is our own).
  Correctness: the reader feeds BEFORE requesting, and the notifier clears
  the pending flag BEFORE delivering, so bytes always land either in the
  snapshot taken for the current wakeup or in a subsequent wakeup — never
  silently dropped.
- child-wait: reaps the child and delivers Exited exactly once. Exited and
  the final Wakeup race by nature; consumers must accept either order.

Locking: the VtTerminal mutex is only ever held for feed/resize and for
VtRenderState::update inside snapshot(). Row/cell readback happens after
update outside the terminal lock, per the ghostty_vt contract. PTY writes
never run on the caller's thread: write_input and the write_pty auto-replies
both queue payloads onto the pty-write channel, so no lock is held across a
blocking write and ordering is the channel's arrival order (matching the
serialization the old writer mutex provided).

Dirty contract: snapshot() consumes BOTH dirty layers (per-row + global)
after copying rows out, so each snapshot's `dirty`/row `dirty` flags describe
changes since the previous snapshot. Rows always carry full content; dirty
flags are a skip-work hint for the renderer, not a completeness marker.
*/

use std::{
    collections::VecDeque,
    io::{Read, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

#[path = "terminal_model/child_lifecycle.rs"]
pub(crate) mod child_lifecycle;
use child_lifecycle::TerminalChild;
#[path = "terminal_model/viewer_detach.rs"]
mod viewer_detach;
pub(crate) use viewer_detach::ViewerDetachPreparation;
use viewer_detach::{ViewerDetachParser, ViewerDetachState};

use crate::ghostty_vt::{
    self, VtCellWide, VtClearScreen, VtDirty, VtError, VtHostCallbacks, VtKeyEncoder, VtKeyInput,
    VtMouseEncoder, VtMouseInput, VtOptionAsAlt, VtRenderState, VtScrollViewport, VtScrollbar,
    VtTerminal, ffi,
};

/// Wakeup coalescing window: bytes arriving within this span of the first
/// unnotified feed produce a single Wakeup.
const WAKEUP_COALESCE_WINDOW: Duration = Duration::from_millis(4);

/// Bound on undrained program-initiated clipboard writes. The consumer
/// drains on every ClipboardWriteRequested event, so the cap only matters
/// when no consumer is pumping events; oldest writes drop first.
const CLIPBOARD_WRITE_QUEUE_LIMIT: usize = 16;

/// PTY read buffer size per read call.
const PTY_READ_BUFFER_LEN: usize = 64 * 1024;

/// Process-local correlation id for temporary paste diagnostics. It connects
/// the main-thread encode/channel handoff to the background PTY write without
/// recording clipboard contents.
static NEXT_PASTE_TRACE_ID: AtomicU64 = AtomicU64::new(1);

static PASTE_DIAGNOSTIC_SINK: OnceLock<TerminalPasteDiagnosticSink> = OnceLock::new();

struct PtyWriteRequest {
    bytes: Vec<u8>,
    paste_trace_id: Option<u64>,
}

pub type Rgb = ffi::GhosttyColorRgb;

/// How to spawn the shell process behind a terminal model.
#[derive(Clone, Debug)]
pub struct TerminalSpawnConfig {
    pub program: String,
    pub args: Vec<String>,
    /// Extra environment on top of the inherited one.
    pub env: Vec<(String, String)>,
    pub cwd: Option<PathBuf>,
    pub cols: u16,
    pub rows: u16,
    pub cell_width_px: u32,
    pub cell_height_px: u32,
    pub max_scrollback: usize,
}

/// Model → consumer notifications. Delivered on model-owned background
/// threads; sinks must be cheap and thread-safe (e.g. post to an executor).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalEvent {
    /// New output was folded into the terminal; take a snapshot when ready.
    Wakeup,
    /// BEL received.
    Bell,
    /// Terminal title changed (OSC 0/2); query lives with P1e.
    TitleChanged,
    /// The running program wrote to the system clipboard (OSC 52 / OSC 1337
    /// Copy). The event carries no content; the consumer drains the queued
    /// text via [`TerminalModel::take_clipboard_write_requests`] and performs
    /// the actual clipboard access on its own thread.
    ClipboardWriteRequested,
    /// Child process exited. Terminal contents stay readable afterwards.
    Exited(TerminalExit),
}

/// Content-free stages of a clipboard paste moving from VT encoding to the
/// background PTY writer. The GPUI host may install a sink; standalone model
/// consumers intentionally run without one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalPasteDiagnostic {
    Encoded {
        trace_id: u64,
        child_pid: Option<u32>,
        bracketed_paste: bool,
        source_byte_length: usize,
        source_contains_line_break: bool,
        source_contains_non_ascii: bool,
        encoded_byte_length: usize,
    },
    ChannelQueued {
        trace_id: u64,
        child_pid: Option<u32>,
        success: bool,
        error_kind: Option<std::io::ErrorKind>,
    },
    PtyWriteStarted {
        trace_id: u64,
        child_pid: Option<u32>,
        encoded_byte_length: usize,
    },
    PtyWriteCompleted {
        trace_id: u64,
        child_pid: Option<u32>,
        duration_micros: u128,
        stage: &'static str,
        success: bool,
        error_kind: Option<std::io::ErrorKind>,
    },
}

pub type TerminalPasteDiagnosticSink = Arc<dyn Fn(TerminalPasteDiagnostic) + Send + Sync>;

pub fn install_paste_diagnostic_sink(sink: TerminalPasteDiagnosticSink) {
    let _ = PASTE_DIAGNOSTIC_SINK.set(sink);
}

fn emit_paste_diagnostic(diagnostic: TerminalPasteDiagnostic) {
    if let Some(sink) = PASTE_DIAGNOSTIC_SINK.get() {
        sink(diagnostic);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalExit {
    /// Process exit code; `None` when waiting on the child itself failed.
    pub code: Option<u32>,
    pub success: bool,
}

pub type TerminalEventSink = Arc<dyn Fn(TerminalEvent) + Send + Sync>;

/// Close-confirmation policy, mirroring the Ghostty `confirm-close-surface`
/// configuration values the app already persists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalConfirmCloseBehavior {
    /// Confirm unless the cursor sits at a shell-integration prompt.
    UnlessPrompt,
    /// Never confirm.
    Never,
    /// Always confirm while the child is alive.
    Always,
}

/// Where a scroll-wheel tick should go, per current terminal modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelRoute {
    /// Mouse tracking is on: encode wheel button events to the PTY.
    Report,
    /// Alt screen + alternateScroll (mode 1007): send arrow keys.
    ArrowKeys,
    /// Primary screen, no tracking: scroll the local scrollback viewport.
    Viewport,
    /// Alt screen without alternateScroll: the wheel does nothing.
    None,
}

/// Underline style of a snapshot cell (SGR 4 / 4:n).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnderlineStyle {
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

/// One rendered cell, fully owned. `None` colors mean "use the snapshot
/// default"; spacer-width cells carry no drawable content but keep the cells
/// vector index-aligned with columns.
#[derive(Clone, Debug)]
pub struct SnapshotCell {
    /// Base character; space for empty cells.
    pub base: char,
    /// Combining characters beyond the base, if any (rare).
    pub combining: Option<Box<str>>,
    pub width: VtCellWide,
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub italic: bool,
    pub faint: bool,
    #[allow(dead_code)]
    // snapshot shape: mirrors the libghostty-vt cell/row attributes even where the gpui renderer ignores them
    pub blink: bool,
    pub inverse: bool,
    pub invisible: bool,
    pub strikethrough: bool,
    pub overline: bool,
    pub underline: UnderlineStyle,
    /// Explicit underline color; `None` means underline uses the cell fg.
    pub underline_color: Option<Rgb>,
    /// Whether the cell carries an OSC 8 hyperlink; the URI is queried on
    /// demand via [`TerminalModel::hyperlink_uri_at_cell`].
    pub has_hyperlink: bool,
}

#[derive(Clone, Debug)]
pub struct SnapshotRow {
    /// Changed since the previous snapshot. Content is always present; this
    /// is a hint that lets the renderer keep cached layout for clean rows.
    pub dirty: bool,
    /// This row continues onto the next row without a hard newline.
    #[allow(dead_code)]
    // snapshot shape: mirrors the libghostty-vt cell/row attributes even where the gpui renderer ignores them
    pub wraps: bool,
    /// This row is the continuation of a soft-wrapped row above it.
    #[allow(dead_code)]
    // snapshot shape: mirrors the libghostty-vt cell/row attributes even where the gpui renderer ignores them
    pub wrap_continuation: bool,
    /// One entry per column, spacers included.
    pub cells: Vec<SnapshotCell>,
}

/// Plain-text scrollback row used by full-buffer search, selection copy, and
/// CLI readback. Rows are keyed by Ghostty's absolute scrollbar row so view
/// scrolling never changes selection/search identity.
#[derive(Clone, Debug)]
pub struct TerminalTextRow {
    pub absolute_row: u64,
    pub wraps: bool,
    #[allow(dead_code)]
    // snapshot shape: mirrors the libghostty-vt cell/row attributes even where the gpui renderer ignores them
    pub wrap_continuation: bool,
    pub cells: Vec<TerminalTextCell>,
}

#[derive(Clone, Debug)]
pub struct TerminalTextCell {
    pub column: u16,
    pub text: String,
}

impl TerminalTextRow {
    #[allow(dead_code)] // used by the terminal-model-smoke / ghostty-vt-smoke binaries
    pub fn text(&self) -> String {
        self.cells.iter().map(|cell| cell.text.as_str()).collect()
    }

    pub fn text_in_columns(&self, start: u16, end: u16) -> String {
        self.cells
            .iter()
            .filter(|cell| cell.column >= start && cell.column < end)
            .map(|cell| cell.text.as_str())
            .collect()
    }
}

impl SnapshotRow {
    /// Row text with spacers skipped and trailing whitespace trimmed.
    /// Convenience for logging/smoke output, not a render path.
    #[allow(dead_code)] // used by the terminal-model-smoke / ghostty-vt-smoke binaries
    pub fn text(&self) -> String {
        let mut text = String::new();
        for cell in &self.cells {
            match cell.width {
                VtCellWide::SpacerTail | VtCellWide::SpacerHead => continue,
                VtCellWide::Narrow | VtCellWide::Wide => {}
            }
            text.push(cell.base);
            if let Some(combining) = &cell.combining {
                text.push_str(combining);
            }
        }
        text.truncate(text.trim_end().len());
        text
    }
}

/// Immutable, fully owned view of one frame. Taking a snapshot consumes the
/// terminal's dirty state, so `dirty`/per-row flags are relative to the
/// previous snapshot; the paint path never touches the terminal lock.
#[derive(Clone, Debug)]
pub struct TerminalSnapshot {
    pub cols: u16,
    /// Viewport rows, top to bottom. Length equals the row count.
    pub rows: Vec<SnapshotRow>,
    /// Global dirty level as of this frame ([`VtDirty::Clean`] means nothing
    /// changed since the previous snapshot).
    pub dirty: VtDirty,
    /// Cursor position in viewport cells, if inside the viewport.
    pub cursor: Option<(u16, u16)>,
    /// DECTCEM cursor visibility.
    pub cursor_visible: bool,
    pub background: Rgb,
    pub foreground: Rgb,
    /// Explicit cursor color, if the terminal set one.
    pub cursor_color: Option<Rgb>,
    /// Active viewport scrollbar state in rows.
    pub scrollbar: VtScrollbar,
    /// Active 256-color palette (for palette-indexed consumers).
    #[allow(dead_code)]
    // snapshot shape: mirrors the libghostty-vt palette even where the gpui renderer resolves colours itself
    pub palette: [Rgb; 256],
}

/// Mirrors `RESTING_GRID_COLS` in `.dependencies/zmx` and must stay equal:
/// it is the column count the zmx daemon rests at while no attached client
/// is displaying, and the width a chat client keeps its local grid at.
pub const ZMX_RESTING_GRID_COLS: u16 = 200;

/// In-band sequence the zmx attach client consumes from its stdin (never
/// forwarded to the shell): "this client is displaying; take the grid at
/// rows x cols".
pub fn zmx_client_visible_sequence(rows: u16, cols: u16) -> String {
    format!("\x1b]1337;ZMX_VISIBLE={rows},{cols}\x07")
}

/// In-band sequence telling the zmx attach client "this client is not
/// displaying a terminal but is showing chat; claim the wide resting grid".
pub fn zmx_client_chat_sequence(rows: u16, cols: u16) -> String {
    format!("\x1b]1337;ZMX_CHAT={rows},{cols}\x07")
}

/// In-band parked claim: retain the daemon grid unless another client needs it.
pub fn zmx_client_hidden_sequence(rows: u16, cols: u16) -> String {
    format!("\x1b]1337;ZMX_HIDDEN={rows},{cols}\x07")
}

/// A live terminal: spawned child on a PTY, libghostty-vt state, background
/// pump threads, and snapshot access. Owned by the UI-side consumer.
pub struct TerminalModel {
    terminal: Arc<Mutex<Option<VtTerminal>>>,
    render_state: Option<VtRenderState>,
    /// Feeds the pty-write thread; sends never block, the thread owns the
    /// PTY write half and performs the actual (possibly blocking) writes.
    write_tx: mpsc::Sender<Option<PtyWriteRequest>>,
    wakeup_tx: mpsc::Sender<Option<()>>,
    pending_input: Arc<AtomicU64>,
    viewer_detach: Arc<Mutex<ViewerDetachState>>,
    event_sink: Arc<Mutex<Option<TerminalEventSink>>>,
    master: Box<dyn MasterPty + Send>,
    child: TerminalChild,
    /// OS pid of the spawned child, for foreground-process liveness checks.
    child_pid: Option<u32>,
    size: (u16, u16),
    cell_size_px: (u32, u32),
    /// Key/mouse encoders (P1d input path). Options re-sync from the live
    /// terminal on every send so encoding always matches the modes the
    /// running program set (kitty flags, DECCKM, tracking mode, ...).
    key_encoder: VtKeyEncoder,
    mouse_encoder: VtMouseEncoder,
    /// Host-owned macOS option-key setting; P1e syncs it from app settings.
    option_as_alt: VtOptionAsAlt,
    /// Program-initiated clipboard writes (OSC 52 / OSC 1337 Copy) queued by
    /// the vt callback until the consumer drains them on
    /// ClipboardWriteRequested. Text only; never logged or persisted.
    clipboard_writes: Arc<Mutex<VecDeque<String>>>,
}

impl TerminalModel {
    /// Spawn the configured process on a fresh PTY and start the pump
    /// threads. Events flow to `events` from background threads immediately.
    pub fn spawn(config: TerminalSpawnConfig, events: TerminalEventSink) -> anyhow::Result<Self> {
        let event_sink = Arc::new(Mutex::new(Some(events)));
        let gated_sink = Arc::clone(&event_sink);
        let events: TerminalEventSink = Arc::new(move |event| {
            if let Some(sink) = gated_sink
                .lock()
                .expect("terminal event lock poisoned")
                .as_ref()
            {
                sink(event);
            }
        });
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(pty_size(
            config.cols,
            config.rows,
            config.cell_width_px,
            config.cell_height_px,
        ))?;

        let mut command = CommandBuilder::new(&config.program);
        crate::terminal_environment::remove_session_identity_from_terminal_command_builder(
            &mut command,
        );
        command.args(&config.args);
        if let Some(cwd) = &config.cwd {
            command.cwd(cwd);
        }
        for (key, value) in &config.env {
            command.env(key, value);
        }
        crate::terminal_environment::apply_color_capable_terminal_command_builder(&mut command);

        let child = pair.slave.spawn_command(command)?;
        // Drop our slave handle so the master sees EOF once the child exits.
        drop(pair.slave);

        let child_pid = child.process_id();
        let mut reader = pair.master.try_clone_reader()?;
        let mut pty_writer = pair.master.take_writer()?;

        // PTY writes run on a dedicated thread: write_input is called from
        // main-thread input handlers, and a stalled PTY (suspended child,
        // XOFF'd tty, full kernel buffer) blocks write_all unboundedly. The
        // thread exits once every sender is gone — the model's write_tx plus
        // the VT reply sender held by the terminal callbacks (dropped once
        // the model and the pty-read thread release the terminal) — and its
        // exit drops the PTY write half.
        let (write_tx, write_rx) = mpsc::channel::<Option<PtyWriteRequest>>();
        let pending_input = Arc::new(AtomicU64::new(0));
        let viewer_detach = Arc::new(Mutex::new(ViewerDetachState::default()));
        let writer_pending_input = Arc::clone(&pending_input);
        let writer_events = Arc::clone(&events);
        let writer_child_pid = child_pid;
        thread::Builder::new()
            .name("ghostex-terminal-pty-write".into())
            .spawn(move || {
                while let Ok(Some(request)) = write_rx.recv() {
                    let started_at = Instant::now();
                    if let Some(trace_id) = request.paste_trace_id {
                        emit_paste_diagnostic(TerminalPasteDiagnostic::PtyWriteStarted {
                            trace_id,
                            child_pid: writer_child_pid,
                            encoded_byte_length: request.bytes.len(),
                        });
                    }

                    let (stage, result) = match pty_writer.write_all(&request.bytes) {
                        Ok(()) => ("flush", pty_writer.flush()),
                        Err(error) => ("write", Err(error)),
                    };
                    if let Some(trace_id) = request.paste_trace_id {
                        emit_paste_diagnostic(TerminalPasteDiagnostic::PtyWriteCompleted {
                            trace_id,
                            child_pid: writer_child_pid,
                            duration_micros: started_at.elapsed().as_micros(),
                            stage,
                            success: result.is_ok(),
                            error_kind: result.as_ref().err().map(std::io::Error::kind),
                        });
                    }
                    if writer_pending_input.fetch_sub(1, Ordering::AcqRel) == 1 {
                        writer_events(TerminalEvent::Wakeup);
                    }
                }
            })?;

        let mut vt = VtTerminal::new(config.cols, config.rows, config.max_scrollback)?;
        let clipboard_writes: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
        {
            // Terminal → host hooks. write_pty fires inside feed() on the
            // pty-read thread while the terminal lock is held; it only
            // queues bytes for the pty-write thread, so no blocking write
            // ever runs under the terminal lock. clipboard_write likewise
            // only queues text; the UI-side consumer performs the actual
            // clipboard access when it handles ClipboardWriteRequested.
            let reply_tx = write_tx.clone();
            let reply_pending_input = Arc::clone(&pending_input);
            let reply_viewer_detach = Arc::clone(&viewer_detach);
            let bell_events = Arc::clone(&events);
            let title_events = Arc::clone(&events);
            let clipboard_events = Arc::clone(&events);
            let clipboard_write_queue = Arc::clone(&clipboard_writes);
            vt.set_host_callbacks(VtHostCallbacks {
                write_pty: Some(Box::new(move |bytes| {
                    let detach = reply_viewer_detach
                        .lock()
                        .expect("terminal detach lock poisoned");
                    if detach.retiring {
                        return;
                    }
                    reply_pending_input.fetch_add(1, Ordering::AcqRel);
                    if reply_tx
                        .send(Some(PtyWriteRequest {
                            bytes: bytes.to_vec(),
                            paste_trace_id: None,
                        }))
                        .is_err()
                    {
                        reply_pending_input.fetch_sub(1, Ordering::AcqRel);
                    }
                })),
                bell: Some(Box::new(move || bell_events(TerminalEvent::Bell))),
                title_changed: Some(Box::new(move || title_events(TerminalEvent::TitleChanged))),
                clipboard_write: Some(Box::new(move |text| {
                    if let Ok(mut queue) = clipboard_write_queue.lock() {
                        if queue.len() >= CLIPBOARD_WRITE_QUEUE_LIMIT {
                            queue.pop_front();
                        }
                        queue.push_back(text);
                    }
                    clipboard_events(TerminalEvent::ClipboardWriteRequested);
                })),
            })?;
        }
        let terminal = Arc::new(Mutex::new(Some(vt)));

        // Wakeup coalescing: `pending` is true while a wakeup is owed but
        // not yet delivered; only the false→true transition signals the
        // notifier, so a whole burst costs one channel send + one event.
        let pending = Arc::new(AtomicBool::new(false));
        let (wakeup_tx, wakeup_rx) = mpsc::channel::<Option<()>>();
        let model_wakeup_tx = wakeup_tx.clone();

        {
            let pending = Arc::clone(&pending);
            let events = Arc::clone(&events);
            thread::Builder::new()
                .name("ghostex-terminal-wakeup".into())
                .spawn(move || {
                    while let Ok(Some(())) = wakeup_rx.recv() {
                        thread::sleep(WAKEUP_COALESCE_WINDOW);
                        // Clear BEFORE delivering: bytes fed after the clear
                        // re-arm the window instead of being folded into a
                        // wakeup the consumer may already be handling.
                        pending.store(false, Ordering::SeqCst);
                        events(TerminalEvent::Wakeup);
                    }
                })?;
        }

        {
            let terminal = Arc::clone(&terminal);
            let pending = Arc::clone(&pending);
            let reader_viewer_detach = Arc::clone(&viewer_detach);
            let reader_events = Arc::clone(&events);
            thread::Builder::new()
                .name("ghostex-terminal-pty-read".into())
                .spawn(move || {
                    let mut buffer = vec![0u8; PTY_READ_BUFFER_LEN];
                    let mut detach_parser = ViewerDetachParser::default();
                    loop {
                        match reader.read(&mut buffer) {
                            // EOF, or EIO once the child side is gone.
                            Ok(0) | Err(_) => break,
                            Ok(len) => {
                                let changed = detach_parser.feed(
                                    &buffer[..len],
                                    &mut reader_viewer_detach
                                        .lock()
                                        .expect("terminal detach lock poisoned"),
                                );
                                if changed {
                                    reader_events(TerminalEvent::Wakeup);
                                }
                                {
                                    let mut terminal =
                                        terminal.lock().expect("terminal lock poisoned");
                                    let Some(terminal) = terminal.as_mut() else {
                                        continue;
                                    };
                                    terminal.feed(&buffer[..len]);
                                }
                                if pending
                                    .compare_exchange(
                                        false,
                                        true,
                                        Ordering::SeqCst,
                                        Ordering::SeqCst,
                                    )
                                    .is_ok()
                                {
                                    let _ = wakeup_tx.send(Some(()));
                                }
                            }
                        }
                    }
                    // Deliver the final pending wakeup, then stop even while
                    // the exited model stays available for readback.
                    let _ = wakeup_tx.send(None);
                })?;
        }

        let child =
            TerminalChild::spawn(child, move |status| events(TerminalEvent::Exited(status)))?;

        Ok(Self {
            terminal,
            render_state: Some(VtRenderState::new()?),
            write_tx,
            wakeup_tx: model_wakeup_tx,
            pending_input,
            viewer_detach,
            event_sink,
            master: pair.master,
            child,
            child_pid,
            size: (config.cols, config.rows),
            cell_size_px: (config.cell_width_px, config.cell_height_px),
            key_encoder: VtKeyEncoder::new()?,
            mouse_encoder: VtMouseEncoder::new()?,
            option_as_alt: VtOptionAsAlt::default(),
            clipboard_writes,
        })
    }

    fn queue_input(&self, request: PtyWriteRequest) -> std::io::Result<()> {
        let mut detach = self
            .viewer_detach
            .lock()
            .expect("terminal detach lock poisoned");
        if self.child.is_detached() || detach.retiring {
            return Err(std::io::ErrorKind::BrokenPipe.into());
        }
        detach.accepted_input = true;
        self.pending_input.fetch_add(1, Ordering::AcqRel);
        if self.write_tx.send(Some(request)).is_err() {
            self.pending_input.fetch_sub(1, Ordering::AcqRel);
            return Err(std::io::ErrorKind::BrokenPipe.into());
        }
        Ok(())
    }

    /// Includes bytes queued on the UI thread and the active PTY write.
    /// A drained queue reports local kernel acceptance, not daemon receipt.
    pub fn has_pending_input(&self) -> bool {
        self.pending_input.load(Ordering::Acquire) != 0
    }

    /// Read-only preflight before creating a replacement display claim.
    pub fn viewer_detach_supported(&self) -> bool {
        if !cfg!(unix) {
            return false;
        }
        let detach = self
            .viewer_detach
            .lock()
            .expect("terminal detach lock poisoned");
        detach.capable
            || detach.acknowledged
            || !detach.accepted_input
            || self.child.exit_status().is_some()
    }

    /// Begin ordered retirement after a replacement display claim is ready.
    pub fn prepare_viewer_detach(&mut self) -> ViewerDetachPreparation {
        #[cfg(not(unix))]
        return ViewerDetachPreparation::Unsupported;
        if self.has_pending_input() {
            return if self
                .viewer_detach
                .lock()
                .expect("terminal detach lock poisoned")
                .retiring
            {
                ViewerDetachPreparation::Pending
            } else {
                ViewerDetachPreparation::Unsupported
            };
        }
        if self.child.exit_status().is_some() {
            return ViewerDetachPreparation::Ready;
        }
        let mut detach = self
            .viewer_detach
            .lock()
            .expect("terminal detach lock poisoned");
        if detach.acknowledged || !detach.accepted_input {
            detach.retiring = true;
            return ViewerDetachPreparation::Ready;
        }
        if detach.nonce.is_some() {
            return ViewerDetachPreparation::Pending;
        }
        if !detach.capable {
            return ViewerDetachPreparation::Unsupported;
        }
        let Ok(bytes) = detach.request() else {
            return ViewerDetachPreparation::Unsupported;
        };
        detach.retiring = true;
        self.pending_input.fetch_add(1, Ordering::AcqRel);
        if self
            .write_tx
            .send(Some(PtyWriteRequest {
                bytes,
                paste_trace_id: None,
            }))
            .is_err()
        {
            self.pending_input.fetch_sub(1, Ordering::AcqRel);
            detach.nonce = None;
            detach.retiring = false;
            return ViewerDetachPreparation::Unsupported;
        }
        ViewerDetachPreparation::Pending
    }

    /// Free emulator and readback memory after this viewer leaves all input/render routes.
    /// The PTY reader keeps draining only the bounded detach-control parser until ACK.
    pub fn release_viewer_emulator(&mut self) {
        self.viewer_detach
            .lock()
            .expect("terminal detach lock poisoned")
            .retiring = true;
        self.terminal.lock().expect("terminal lock poisoned").take();
        self.render_state.take();
        let _ = self.wakeup_tx.send(None);
        self.clipboard_writes
            .lock()
            .expect("terminal clipboard lock poisoned")
            .clear();
    }

    /// Retire a confirmed daemon-backed attach viewer, not an owning shell PTY.
    /// The caller removes its event subscription first and waits for pending input.
    pub fn detach_viewer(&mut self) -> std::io::Result<()> {
        if self.child.is_detached() {
            return Ok(());
        }
        if !cfg!(unix) {
            return Err(std::io::ErrorKind::Unsupported.into());
        }
        let detach = self
            .viewer_detach
            .lock()
            .expect("terminal detach lock poisoned");
        if self.has_pending_input()
            || (detach.accepted_input && !detach.acknowledged && self.child.exit_status().is_none())
        {
            return Err(std::io::ErrorKind::WouldBlock.into());
        }
        drop(detach);
        self.event_sink
            .lock()
            .expect("terminal event lock poisoned")
            .take();
        #[cfg(unix)]
        let foreground_group = self.master.process_group_leader();
        #[cfg(not(unix))]
        let foreground_group = None;
        let write_tx = self.write_tx.clone();
        self.child.detach_viewer(foreground_group, move || {
            let _ = write_tx.send(None);
        })
    }

    /// Drain pending program-initiated clipboard writes (OSC 52 / OSC 1337
    /// Copy), oldest first. The caller performs the actual clipboard access;
    /// the model never touches the system clipboard itself.
    pub fn take_clipboard_write_requests(&self) -> Vec<String> {
        self.clipboard_writes
            .lock()
            .map(|mut queue| queue.drain(..).collect())
            .unwrap_or_default()
    }

    /// Queue input bytes (encoded key/mouse/paste data) for the PTY. The
    /// actual write happens on the pty-write thread, so callers (main-thread
    /// input handlers) never block on a stalled PTY.
    pub fn write_input(&self, bytes: &[u8]) -> std::io::Result<()> {
        self.queue_input(PtyWriteRequest {
            bytes: bytes.to_vec(),
            paste_trace_id: None,
        })
    }

    /// Encode a key event against the terminal's live keyboard modes and
    /// write it to the PTY. Returns true when the event produced bytes (the
    /// caller should treat the key as handled), false when the encoder had
    /// nothing to send (bare modifiers, unbound cmd shortcuts, ...).
    pub fn send_key(&mut self, input: &VtKeyInput<'_>) -> bool {
        self.send_key_repeat(input, 1)
    }

    /// Send the same key `count` times as one PTY write, the way ghostty
    /// queues a wheel notch's worth of cursor keys onto a single io write:
    /// the program reads the whole burst in one go instead of repainting
    /// between rows.
    pub fn send_key_repeat(&mut self, input: &VtKeyInput<'_>, count: usize) -> bool {
        if count == 0 {
            return false;
        }
        {
            let mut terminal = self.terminal.lock().expect("terminal lock poisoned");
            let Some(terminal) = terminal.as_mut() else {
                return false;
            };
            self.key_encoder
                .sync_from_terminal(terminal, self.option_as_alt);
        }
        let mut bytes = Vec::new();
        if self.key_encoder.encode(input, &mut bytes).is_err() || bytes.is_empty() {
            return false;
        }
        let _ = self.write_input(&bytes.repeat(count));
        true
    }

    /// Clear the screen and scrollback the way ghostty's `clear_screen`
    /// binding does, sending the shell a form feed when the clear happened
    /// at a prompt so it repaints. Returns false when nothing was cleared
    /// (the alternate screen is left alone): ghostty marks that binding
    /// `performable`, so the caller lets the key reach the program instead.
    pub fn clear_screen(&mut self) -> bool {
        let outcome = self
            .terminal
            .lock()
            .expect("terminal lock poisoned")
            .as_mut()
            .and_then(|terminal| terminal.clear_screen(true).ok())
            .unwrap_or(VtClearScreen::NotCleared);
        match outcome {
            VtClearScreen::NotCleared => false,
            VtClearScreen::Cleared => true,
            VtClearScreen::ClearedAtPrompt => {
                let _ = self.write_input(&[0x0c]);
                true
            }
        }
    }

    /// Active Kitty keyboard-protocol flags used by the next encoded key.
    pub fn kitty_keyboard_flags(&self) -> Option<u8> {
        self.terminal
            .lock()
            .ok()
            .and_then(|mut terminal| terminal.as_mut()?.kitty_keyboard_flags().ok())
    }

    /// Encode a mouse event against the terminal's live tracking mode and
    /// write it to the PTY. Returns true when bytes were sent; false means
    /// the active tracking mode does not report this event (or none is on).
    /// Positions are DEVICE pixels relative to the grid origin, matching the
    /// cell pixel sizes given to [`resize`](Self::resize).
    pub fn send_mouse(&mut self, input: &VtMouseInput, any_button_pressed: bool) -> bool {
        self.send_mouse_repeat(input, any_button_pressed, 1)
    }

    /// Send the same mouse event `count` times as one PTY write. Wheel
    /// reports are the only repeated event, and ghostty likewise queues a
    /// notch's reports together.
    pub fn send_mouse_repeat(
        &mut self,
        input: &VtMouseInput,
        any_button_pressed: bool,
        count: usize,
    ) -> bool {
        if count == 0 {
            return false;
        }
        {
            let mut terminal = self.terminal.lock().expect("terminal lock poisoned");
            let Some(terminal) = terminal.as_mut() else {
                return false;
            };
            self.mouse_encoder.sync_from_terminal(terminal);
        }
        let (cols, rows) = self.size;
        let (cell_width_px, cell_height_px) = self.cell_size_px;
        self.mouse_encoder.set_size(
            u32::from(cols) * cell_width_px,
            u32::from(rows) * cell_height_px,
            cell_width_px,
            cell_height_px,
        );
        self.mouse_encoder
            .set_any_button_pressed(any_button_pressed);
        let mut bytes = Vec::new();
        if self.mouse_encoder.encode(input, &mut bytes).is_err() || bytes.is_empty() {
            return false;
        }
        let _ = self.write_input(&bytes.repeat(count));
        true
    }

    /// Write clipboard text to the PTY, honoring bracketed paste mode.
    pub fn send_paste(&mut self, text: &str) -> std::io::Result<()> {
        let bracketed = self.mode_active(ffi::GHOSTTY_MODE_BRACKETED_PASTE);
        let bytes = ghostty_vt::encode_paste(text, bracketed)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        let trace_id = NEXT_PASTE_TRACE_ID.fetch_add(1, Ordering::Relaxed);
        emit_paste_diagnostic(TerminalPasteDiagnostic::Encoded {
            trace_id,
            child_pid: self.child_pid,
            bracketed_paste: bracketed,
            source_byte_length: text.len(),
            source_contains_line_break: text.contains(['\r', '\n']),
            source_contains_non_ascii: !text.is_ascii(),
            encoded_byte_length: bytes.len(),
        });
        let result = self.queue_input(PtyWriteRequest {
            bytes,
            paste_trace_id: Some(trace_id),
        });
        emit_paste_diagnostic(TerminalPasteDiagnostic::ChannelQueued {
            trace_id,
            child_pid: self.child_pid,
            success: result.is_ok(),
            error_kind: result.as_ref().err().map(std::io::Error::kind),
        });
        result
    }

    /// Report a focus change to the PTY when focus reporting (mode 1004) is
    /// active; silent otherwise.
    pub fn send_focus(&mut self, focused: bool) {
        if !self.mode_active(ffi::GHOSTTY_MODE_FOCUS_EVENT) {
            return;
        }
        if let Ok(bytes) = ghostty_vt::encode_focus(focused) {
            let _ = self.write_input(&bytes);
        }
    }

    /// Whether the macOS option key acts as alt for key encoding. When
    /// false (default), option-modified text keys belong to the IME/text
    /// insertion path instead of the encoder.
    pub fn option_sends_alt(&self) -> bool {
        self.option_as_alt != VtOptionAsAlt::False
    }

    /// Whether any mouse tracking mode is active (the PTY owns mouse input;
    /// shift bypasses this by convention for local selection).
    pub fn mouse_tracking(&mut self) -> bool {
        self.terminal
            .lock()
            .expect("terminal lock poisoned")
            .as_mut()
            .is_some_and(|terminal| terminal.mouse_tracking().unwrap_or(false))
    }

    /// Current value of a terminal mode (`ffi::GHOSTTY_MODE_*`).
    pub fn mode_active(&mut self, mode: ffi::GhosttyMode) -> bool {
        self.terminal
            .lock()
            .expect("terminal lock poisoned")
            .as_mut()
            .is_some_and(|terminal| terminal.mode(mode).unwrap_or(false))
    }

    /// How a wheel tick should be delivered given the current terminal
    /// modes, resolved under one terminal lock.
    pub fn wheel_route(&mut self) -> WheelRoute {
        let mut terminal = self.terminal.lock().expect("terminal lock poisoned");
        let Some(terminal) = terminal.as_mut() else {
            return WheelRoute::None;
        };
        if terminal.mouse_tracking().unwrap_or(false) {
            return WheelRoute::Report;
        }
        if terminal.alternate_screen_active().unwrap_or(false) {
            // xterm alternateScroll: wheel becomes arrow keys on the alt
            // screen when mode 1007 is set; otherwise the alt screen has no
            // scrollback so the wheel does nothing.
            return if terminal.mode(ffi::GHOSTTY_MODE_ALT_SCROLL).unwrap_or(false) {
                WheelRoute::ArrowKeys
            } else {
                WheelRoute::None
            };
        }
        WheelRoute::Viewport
    }

    /// Scroll the scrollback viewport (primary screen only; no-op without
    /// scrollback). Take a fresh snapshot afterwards to render the change.
    pub fn scroll_viewport(&mut self, behavior: VtScrollViewport) {
        self.terminal
            .lock()
            .expect("terminal lock poisoned")
            .as_mut()
            .map(|terminal| terminal.scroll_viewport(behavior));
    }

    /// Current OSC 0/2 title, if the running program set one.
    pub fn title(&mut self) -> Option<String> {
        self.terminal
            .lock()
            .expect("terminal lock poisoned")
            .as_mut()
            .and_then(|terminal| terminal.title().ok().flatten())
    }

    /// Current OSC 7 working directory, if the running program reported one.
    pub fn pwd(&mut self) -> Option<String> {
        self.terminal
            .lock()
            .expect("terminal lock poisoned")
            .as_mut()
            .and_then(|terminal| terminal.pwd().ok().flatten())
    }

    /// OSC 8 hyperlink URI at a viewport cell, if any. Cheap enough for
    /// hover-time queries; not a render-loop API.
    pub fn hyperlink_uri_at_cell(&mut self, col: u16, row: u16) -> Option<String> {
        self.terminal
            .lock()
            .expect("terminal lock poisoned")
            .as_mut()
            .and_then(|terminal| terminal.hyperlink_uri_at_viewport(col, row).ok().flatten())
    }

    /// Configure how the macOS option key participates in key encoding.
    pub fn set_option_as_alt(&mut self, option_as_alt: VtOptionAsAlt) {
        self.option_as_alt = option_as_alt;
    }

    pub fn set_default_colors(
        &mut self,
        foreground: Rgb,
        background: Rgb,
        cursor: Option<Rgb>,
        palette: &[Rgb; 256],
    ) -> Result<(), VtError> {
        self.terminal
            .lock()
            .expect("terminal lock poisoned")
            .as_mut()
            .ok_or(VtError {
                code: ffi::GHOSTTY_INVALID_VALUE,
            })?
            .set_default_colors(foreground, background, cursor, palette)
    }

    /// Whether closing this terminal should ask for confirmation, mirroring
    /// ghostty `Surface.needsConfirmQuit`: an exited child never confirms;
    /// otherwise `confirm-close-surface` semantics apply, with the `true`
    /// value skipping confirmation while the cursor sits at a
    /// shell-integration prompt.
    pub fn needs_confirm_close(&mut self, behavior: TerminalConfirmCloseBehavior) -> bool {
        if self.exit_status().is_some() {
            return false;
        }
        match behavior {
            TerminalConfirmCloseBehavior::Always => true,
            TerminalConfirmCloseBehavior::Never => false,
            TerminalConfirmCloseBehavior::UnlessPrompt => !self
                .terminal
                .lock()
                .expect("terminal lock poisoned")
                .as_mut()
                .is_some_and(|terminal| terminal.cursor_at_prompt().unwrap_or(false)),
        }
    }

    /// Whether a process other than the spawned child owns the PTY
    /// foreground (e.g. an editor launched from the shell). Complements the
    /// prompt check above for hosts that want native-style liveness info.
    /// Unix-only concept (foreground process groups); Windows reports false.
    #[allow(dead_code)] // public TerminalModel API kept complete alongside kill()
    pub fn foreground_process_active(&self) -> bool {
        #[cfg(unix)]
        {
            match (self.master.process_group_leader(), self.child_pid) {
                (Some(foreground), Some(child)) => i64::from(foreground) != i64::from(child),
                _ => false,
            }
        }
        #[cfg(not(unix))]
        {
            false
        }
    }

    /// Propagate a cell-grid size change to the vt terminal and the PTY
    /// (TIOCSWINSZ + SIGWINCH via portable-pty). The vt terminal resizes
    /// first so redraw output triggered by SIGWINCH meets the new grid.
    pub fn resize(
        &mut self,
        cols: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> anyhow::Result<()> {
        if (cols, rows) == self.size && (cell_width_px, cell_height_px) == self.cell_size_px {
            return Ok(());
        }
        self.terminal
            .lock()
            .expect("terminal lock poisoned")
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("terminal viewer retired"))?
            .resize(cols, rows, cell_width_px, cell_height_px)?;
        self.master
            .resize(pty_size(cols, rows, cell_width_px, cell_height_px))?;
        self.size = (cols, rows);
        self.cell_size_px = (cell_width_px, cell_height_px);
        Ok(())
    }

    /// Resize the grid keeping the current cell pixel sizes, for callers
    /// that change the cell grid without a layout pass (a parked terminal
    /// resting at the zmx daemon's wide grid).
    pub fn resize_grid(&mut self, cols: u16, rows: u16) -> anyhow::Result<()> {
        let (cell_width_px, cell_height_px) = self.cell_size_px;
        self.resize(cols, rows, cell_width_px, cell_height_px)
    }

    /// Grid size in cells as `(cols, rows)`.
    pub fn size(&self) -> (u16, u16) {
        self.size
    }

    /// Spawned child pid used only to correlate content-free runtime
    /// diagnostics across the terminal view and PTY writer.
    pub fn diagnostic_child_pid(&self) -> Option<u32> {
        self.child_pid
    }

    /// Exit status once the child has exited.
    pub fn exit_status(&self) -> Option<TerminalExit> {
        self.child.exit_status()
    }

    /// Terminate the child process (SIGHUP/kill semantics per platform).
    #[allow(dead_code)] // public TerminalModel API kept complete alongside foreground_process_active()
    pub fn kill(&mut self) -> std::io::Result<()> {
        self.child.kill()
    }

    /// Move the viewport to an absolute scrollbar row. This is the same row
    /// coordinate Ghostty reports through `scrollbar.offset` and is used by
    /// search navigation and the interactive scrollbar.
    pub fn scroll_viewport_to_row(&mut self, row: u64) {
        let mut terminal = self.terminal.lock().expect("terminal lock poisoned");
        let Some(terminal) = terminal.as_mut() else {
            return;
        };
        let Ok(scrollbar) = terminal.scrollbar() else {
            return;
        };
        let target = row.min(scrollbar.total.saturating_sub(scrollbar.len));
        let delta = i128::from(target) - i128::from(scrollbar.offset);
        scroll_terminal_delta(terminal, delta);
    }

    /// Current scrollbar state (viewport position, in rows) without consuming
    /// snapshot dirty state.
    pub fn scrollbar(&mut self) -> Result<VtScrollbar, VtError> {
        let mut terminal = self.terminal.lock().expect("terminal lock poisoned");
        let Some(terminal) = terminal.as_mut() else {
            return Err(VtError {
                code: ffi::GHOSTTY_INVALID_VALUE,
            });
        };
        terminal.scrollbar()
    }

    /// Read every scrollback row without changing the user's final viewport.
    /// The terminal lock stays held so output cannot reorder absolute rows
    /// during the scan. Render-state snapshots are page-sized; walk them from
    /// top to bottom, then restore the exact original offset.
    pub fn read_scrollback_rows(&mut self) -> Result<Vec<TerminalTextRow>, VtError> {
        let mut terminal = self.terminal.lock().expect("terminal lock poisoned");
        let terminal = terminal.as_mut().ok_or(VtError {
            code: ffi::GHOSTTY_INVALID_VALUE,
        })?;
        let render_state = self.render_state.as_mut().ok_or(VtError {
            code: ffi::GHOSTTY_INVALID_VALUE,
        })?;
        let original = terminal.scrollbar()?;
        terminal.scroll_viewport(VtScrollViewport::Top);

        let result = (|| {
            let mut output = Vec::with_capacity(original.total.min(usize::MAX as u64) as usize);
            let mut next_absolute_row = 0_u64;
            loop {
                render_state.update(terminal)?;
                let scrollbar = terminal.scrollbar()?;
                let mut viewport_index = 0_u64;
                let mut rows = render_state.rows()?;
                while let Some(mut row) = rows.next_row() {
                    let absolute_row = scrollbar.offset.saturating_add(viewport_index);
                    viewport_index = viewport_index.saturating_add(1);
                    if absolute_row < next_absolute_row || absolute_row >= scrollbar.total {
                        continue;
                    }
                    let wraps = row.wraps()?;
                    let wrap_continuation = row.wrap_continuation()?;
                    let mut cells_out = Vec::new();
                    let mut column = 0_u16;
                    let mut codepoints = Vec::new();
                    let mut cells = row.cells()?;
                    while let Some(cell) = cells.next_cell() {
                        let width = cell.wide()?;
                        codepoints.clear();
                        cell.append_codepoints(&mut codepoints)?;
                        if !matches!(width, VtCellWide::SpacerTail | VtCellWide::SpacerHead) {
                            let text = if codepoints.is_empty() {
                                " ".to_string()
                            } else {
                                codepoints
                                    .iter()
                                    .map(|codepoint| {
                                        char::from_u32(*codepoint)
                                            .unwrap_or(char::REPLACEMENT_CHARACTER)
                                    })
                                    .collect()
                            };
                            cells_out.push(TerminalTextCell { column, text });
                        }
                        column = column.saturating_add(1);
                    }
                    output.push(TerminalTextRow {
                        absolute_row,
                        wraps,
                        wrap_continuation,
                        cells: cells_out,
                    });
                    next_absolute_row = absolute_row.saturating_add(1);
                }
                drop(rows);
                if scrollbar.offset.saturating_add(scrollbar.len) >= scrollbar.total
                    || scrollbar.len == 0
                {
                    break;
                }
                terminal.scroll_viewport(VtScrollViewport::Delta(
                    scrollbar.len.min(isize::MAX as u64) as isize,
                ));
            }
            Ok(output)
        })();

        terminal.scroll_viewport(VtScrollViewport::Top);
        scroll_terminal_delta(terminal, i128::from(original.offset));
        let _ = render_state.update(terminal);
        result
    }

    /// Take an owned frame snapshot. Holds the terminal lock only for the
    /// render-state update; row/cell copy-out and dirty clearing run outside
    /// it. Consumes both dirty layers per the ghostty_vt contract.
    pub fn snapshot(&mut self) -> Result<TerminalSnapshot, VtError> {
        let render_state = self.render_state.as_mut().ok_or(VtError {
            code: ffi::GHOSTTY_INVALID_VALUE,
        })?;
        let scrollbar = {
            let mut terminal = self.terminal.lock().expect("terminal lock poisoned");
            let terminal = terminal.as_mut().ok_or(VtError {
                code: ffi::GHOSTTY_INVALID_VALUE,
            })?;
            render_state.update(terminal)?;
            terminal.scrollbar()?
        };

        let (cols, rows) = render_state.size()?;
        let dirty = render_state.dirty()?;
        let colors = render_state.colors()?;
        let cursor = render_state.cursor_viewport()?;
        let cursor_visible = render_state.cursor_visible()?;

        let mut snapshot_rows: Vec<SnapshotRow> = Vec::with_capacity(rows as usize);
        let mut codepoints: Vec<u32> = Vec::new();
        let mut row_iter = render_state.rows()?;
        while let Some(mut row) = row_iter.next_row() {
            let row_dirty = row.is_dirty()?;
            let mut cells: Vec<SnapshotCell> = Vec::with_capacity(cols as usize);
            {
                let mut cell_iter = row.cells()?;
                while let Some(cell) = cell_iter.next_cell() {
                    codepoints.clear();
                    cell.append_codepoints(&mut codepoints)?;
                    let mut chars = codepoints.iter().map(|codepoint| {
                        char::from_u32(*codepoint).unwrap_or(char::REPLACEMENT_CHARACTER)
                    });
                    let base = chars.next().unwrap_or(' ');
                    let combining: Option<Box<str>> = if codepoints.len() > 1 {
                        Some(chars.collect::<String>().into_boxed_str())
                    } else {
                        None
                    };
                    let style = cell.style()?;
                    cells.push(SnapshotCell {
                        base,
                        combining,
                        width: cell.wide()?,
                        fg: cell.fg_color()?,
                        bg: cell.bg_color()?,
                        bold: style.bold,
                        italic: style.italic,
                        faint: style.faint,
                        blink: style.blink,
                        inverse: style.inverse,
                        invisible: style.invisible,
                        strikethrough: style.strikethrough,
                        overline: style.overline,
                        underline: underline_style(style.underline),
                        underline_color: ghostty_vt::style_color_rgb(
                            &style.underline_color,
                            &colors.palette,
                        ),
                        has_hyperlink: cell.has_hyperlink()?,
                    });
                }
            }
            row.clear_dirty()?;
            snapshot_rows.push(SnapshotRow {
                dirty: row_dirty,
                wraps: row.wraps()?,
                wrap_continuation: row.wrap_continuation()?,
                cells,
            });
        }
        drop(row_iter);
        render_state.clear_dirty()?;

        Ok(TerminalSnapshot {
            cols,
            rows: snapshot_rows,
            dirty,
            cursor,
            cursor_visible,
            background: colors.background,
            foreground: colors.foreground,
            cursor_color: colors.cursor_has_value.then_some(colors.cursor),
            scrollbar,
            palette: colors.palette,
        })
    }
}

impl Drop for TerminalModel {
    fn drop(&mut self) {
        if self.child.is_detached() {
            return;
        }
        let retiring = self
            .viewer_detach
            .lock()
            .expect("terminal detach lock poisoned")
            .retiring;
        if retiring {
            #[cfg(unix)]
            let foreground_group = self.master.process_group_leader();
            #[cfg(not(unix))]
            let foreground_group = None;
            let write_tx = self.write_tx.clone();
            if self
                .child
                .detach_viewer(foreground_group, move || {
                    let _ = write_tx.send(None);
                })
                .is_err()
            {
                // Keep the writer from injecting newline/VEOF if disposal cannot
                // be scheduled. Ordinary single-child cleanup still runs below.
                std::mem::forget(self.write_tx.clone());
                let _ = self.child.kill();
            }
        } else {
            // portable-pty's writer Drop writes newline/VEOF. Preserve the
            // original ordering: terminate the child before dropping senders.
            let _ = self.child.kill();
        }
    }
}

fn scroll_terminal_delta(terminal: &mut ghostty_vt::VtTerminal, mut delta: i128) {
    while delta != 0 {
        let step = delta.clamp(-(isize::MAX as i128), isize::MAX as i128) as isize;
        terminal.scroll_viewport(VtScrollViewport::Delta(step));
        delta -= step as i128;
    }
}

fn pty_size(cols: u16, rows: u16, cell_width_px: u32, cell_height_px: u32) -> PtySize {
    PtySize {
        rows,
        cols,
        pixel_width: (u32::from(cols) * cell_width_px).min(u32::from(u16::MAX)) as u16,
        pixel_height: (u32::from(rows) * cell_height_px).min(u32::from(u16::MAX)) as u16,
    }
}

fn underline_style(raw: ffi::GhosttySgrUnderline) -> UnderlineStyle {
    match raw {
        ffi::GHOSTTY_SGR_UNDERLINE_SINGLE => UnderlineStyle::Single,
        ffi::GHOSTTY_SGR_UNDERLINE_DOUBLE => UnderlineStyle::Double,
        ffi::GHOSTTY_SGR_UNDERLINE_CURLY => UnderlineStyle::Curly,
        ffi::GHOSTTY_SGR_UNDERLINE_DOTTED => UnderlineStyle::Dotted,
        ffi::GHOSTTY_SGR_UNDERLINE_DASHED => UnderlineStyle::Dashed,
        _ => UnderlineStyle::None,
    }
}
