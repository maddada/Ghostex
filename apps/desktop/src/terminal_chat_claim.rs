use std::{
    io::{self, Read, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread,
};

use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::terminal_model::{
    TerminalSpawnConfig, ZMX_RESTING_GRID_COLS, child_lifecycle::TerminalChild,
    zmx_client_chat_sequence,
};

const ATTACH_READY: &[u8] = b"\x1b[2J\x1b[H";
const ATTACH_RESET: &[u8] = b"\x1bc\x1b]110";
const SCAN_CARRY: usize = 7;
type Writer = Box<dyn Write + Send>;

enum Input {
    Bytes(Vec<u8>),
    Claim,
    Stop,
}

/// CDXC:Zmx 2026-09-13 WHY:
/// Chat sizing and focus presence belong to a live zmx attach client, not to its emulator.
/// Keep that lightweight client only while native layout displays Chat; dropping the claim closes the attach transport without touching the daemon or its agent.
/// SEE-ALSO: .dependencies/zmx/src/main.zig attach raw-mode marker, .dependencies/zmx/src/loop.zig handleVisibility and last-client focus reporting.
pub(crate) struct TerminalChatClaim {
    child: TerminalChild,
    master: Box<dyn MasterPty + Send>,
    writer: Arc<Mutex<Option<Writer>>>,
    input: SyncSender<Input>,
    stopped: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
}

impl TerminalChatClaim {
    /// The configuration must be the authoritative local or SSH zmx attach recipe.
    /// Initial user input belongs to a viewer lease and must never enter this owner.
    pub(crate) fn spawn(mut config: TerminalSpawnConfig) -> io::Result<Self> {
        config.cols = ZMX_RESTING_GRID_COLS;
        config.rows = config.rows.max(1);
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: config.rows,
                cols: config.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| io::Error::other(error.to_string()))?;
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| io::Error::other(error.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| io::Error::other(error.to_string()))?;
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
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| io::Error::other(error.to_string()))?;
        drop(pair.slave);
        let stopped = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let ready = Arc::new(AtomicBool::new(false));
        let child_ready = Arc::clone(&ready);
        let child = TerminalChild::spawn(child, move |_| {
            child_ready.store(false, Ordering::Release);
        })?;
        let (input, incoming) = mpsc::sync_channel(8);
        let owner = Self {
            child,
            master: pair.master,
            writer: Arc::new(Mutex::new(Some(writer))),
            input,
            stopped,
            failed,
            ready,
        };

        let writer = Arc::clone(&owner.writer);
        let ready = Arc::clone(&owner.ready);
        let failed = Arc::clone(&owner.failed);
        let stopped = Arc::clone(&owner.stopped);
        let claim = zmx_client_chat_sequence(config.rows, config.cols);
        thread::Builder::new()
            .name("ghostex-chat-claim-write".into())
            .spawn(move || {
                // Keep the writer in the shared owner until explicit termination.
                // portable-pty injects newline/VEOF when its Unix writer drops.
                while let Ok(request) = incoming.recv() {
                    if matches!(request, Input::Stop) {
                        break;
                    }
                    if stopped.load(Ordering::Acquire) {
                        continue;
                    }
                    let is_claim = matches!(request, Input::Claim);
                    let bytes = match &request {
                        Input::Bytes(bytes) => bytes.as_slice(),
                        Input::Claim => claim.as_bytes(),
                        Input::Stop => unreachable!(),
                    };
                    let mut guard = writer.lock().expect("chat claim writer lock poisoned");
                    let Some(writer) = guard.as_mut() else {
                        break;
                    };
                    if writer
                        .write_all(bytes)
                        .and_then(|()| writer.flush())
                        .is_err()
                    {
                        failed.store(true, Ordering::Release);
                        ready.store(false, Ordering::Release);
                        break;
                    }
                    if is_claim {
                        ready.store(true, Ordering::Release);
                    }
                }
            })?;
        let input = owner.input.clone();
        let stopped = Arc::clone(&owner.stopped);
        let ready = Arc::clone(&owner.ready);
        let failed = Arc::clone(&owner.failed);
        thread::Builder::new()
            .name("ghostex-chat-claim-read".into())
            .spawn(move || drain_output(reader, input, stopped, ready, failed))?;
        Ok(owner)
    }

    pub(crate) fn is_alive(&self) -> bool {
        !self.failed.load(Ordering::Acquire)
            && !self.child.is_detached()
            && self.child.exit_status().is_none()
    }

    /// The raw-mode attach marker was observed and the Chat claim was written.
    /// Existing zmx attach stdout does not expose a daemon acknowledgement.
    pub(crate) fn is_ready(&self) -> bool {
        self.is_alive() && self.ready.load(Ordering::Acquire)
    }
}

impl Drop for TerminalChatClaim {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        self.ready.store(false, Ordering::Release);
        #[cfg(unix)]
        let foreground_group = self.master.process_group_leader();
        #[cfg(not(unix))]
        let foreground_group = None;
        let input = self.input.clone();
        let writer = Arc::clone(&self.writer);
        if self
            .child
            .detach_viewer(foreground_group, move || {
                let _ = input.send(Input::Stop);
                writer
                    .lock()
                    .expect("chat claim writer lock poisoned")
                    .take();
            })
            .is_err()
        {
            // If a disposal thread cannot be started, retaining this handle is
            // safer than injecting EOF into a still-connected agent transport.
            std::mem::forget(Arc::clone(&self.writer));
        }
    }
}

fn drain_output(
    mut reader: Box<dyn Read + Send>,
    input: SyncSender<Input>,
    stopped: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
) {
    let mut buffer = [0_u8; 8192 + SCAN_CARRY];
    let mut carry = 0;
    loop {
        let count = match reader.read(&mut buffer[carry..]) {
            Ok(0) | Err(_) => break,
            Ok(count) => count,
        };
        if stopped.load(Ordering::Acquire) {
            return;
        }
        let length = carry + count;
        let bytes = &buffer[..length];
        let mut replies = Vec::new();
        for start in 0..length {
            let tail = &bytes[start..];
            let complete =
                |pattern: &[u8]| start + pattern.len() > carry && tail.starts_with(pattern);
            if complete(ATTACH_RESET) {
                ready.store(false, Ordering::Release);
            }
            if complete(ATTACH_READY) && input.try_send(Input::Claim).is_err() {
                failed.store(true, Ordering::Release);
                return;
            }
            // An Init client disables zmx's own headless DA responder. Answer
            // the same bounded DA1/DA2 queries without interpreting a VT grid.
            // SEE-ALSO: .dependencies/zmx/src/util.zig respondToDeviceAttributes.
            if complete(b"\x1b[>c") || complete(b"\x1b[>0c") {
                replies.extend_from_slice(b"\x1b[>1;10;0c");
            } else if complete(b"\x1b[c") || complete(b"\x1b[0c") {
                replies.extend_from_slice(b"\x1b[?62;22c");
            }
        }
        if !replies.is_empty() {
            let _ = input.try_send(Input::Bytes(replies));
        }
        carry = length.min(SCAN_CARRY);
        buffer.copy_within(length - carry..length, 0);
    }
    failed.store(true, Ordering::Release);
    ready.store(false, Ordering::Release);
}
