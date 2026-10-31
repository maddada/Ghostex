//! Running a command to its end within a time limit, with optional input and a bound on how much
//! of its output is kept (`run_bounded`).

use std::{
    io::{Read, Write},
    process::{Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use super::ProcessTree;

/// How much of one output stream `run_bounded` keeps.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Keep {
    /// The first `n` bytes, give or take one read past them; the rest is read and dropped without
    /// a word, so a runaway printer is never kept whole and never blocks.
    Head(usize),
    /// The last `n` bytes.
    Tail(usize),
    /// Up to `n` bytes, read to its end. One more byte fails the run with `Unfinished::TooLarge`,
    /// and a stream still open at the time limit (a child the command left behind holds it)
    /// fails it with `Unfinished::TimedOut`; either ends the command with everything it started.
    /// So a cut answer can never pass for a whole one.
    AtMost(usize),
}

/// What `run_bounded` gives a command and keeps of what it prints.
pub(crate) struct RunLimits {
    pub(crate) timeout: Duration,
    /// Written to the command's stdin, which is then closed. `None` gives it no input at all.
    pub(crate) stdin: Option<Vec<u8>>,
    pub(crate) stdout: Keep,
    pub(crate) stderr: Keep,
}

/// What a bounded command printed before it exited.
pub(crate) struct RunOutput {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

/// Only the status and how much each stream held: output can carry a credential the command was
/// given.
impl std::fmt::Debug for RunOutput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RunOutput")
            .field("status", &self.status)
            .field("stdout_bytes", &self.stdout.len())
            .field("stderr_bytes", &self.stderr.len())
            .finish()
    }
}

#[derive(Debug)]
pub(crate) enum Unfinished {
    /// It could not start at all (missing, not executable).
    Start(std::io::Error),
    /// It ran past its time limit, or a `Keep::AtMost` stream was still open then, and was
    /// stopped with everything it started.
    TimedOut(Duration),
    /// A stream kept with `Keep::AtMost(n)` passed `n` bytes; it was stopped with everything it
    /// started.
    TooLarge(usize),
}

/// Runs `command` within `limits.timeout`, then stops it and every process it started. The
/// command leads its own process tree (`ProcessTree`), so a wrapper's children go too. A child
/// the command left behind holding its output open gets what is left of the time limit, then is
/// ended (one still holding its input too); what was printed until then is kept, except from a
/// `Keep::AtMost` stream, whose answer is then cut short and fails the run.
pub(crate) fn run_bounded(
    mut command: Command,
    limits: RunLimits,
) -> Result<RunOutput, Unfinished> {
    command
        .stdin(if limits.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut tree = ProcessTree::spawn(&mut command).map_err(Unfinished::Start)?;
    // On a thread of its own: a command that fills its stdout before it reads its input would
    // otherwise wait on this write while it waits on the command. The write ends once the input is
    // in the pipe, or when the last process holding the pipe's other end exits.
    let writer = match (limits.stdin, tree.child().stdin.take()) {
        (Some(input), Some(mut pipe)) => Some(std::thread::spawn(move || {
            let _ = pipe.write_all(&input);
        })),
        _ => None,
    };
    let writer_finished = || writer.as_ref().is_none_or(JoinHandle::is_finished);
    let stdout = Collected::start(tree.child().stdout.take(), limits.stdout);
    let stderr = Collected::start(tree.child().stderr.take(), limits.stderr);
    let overran = || stdout.overran().or_else(|| stderr.overran());
    let deadline = Instant::now() + limits.timeout;
    let status = loop {
        if let Some(cap) = overran() {
            tree.end();
            return Err(Unfinished::TooLarge(cap));
        }
        match tree.child().try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                tree.end();
                return Err(Unfinished::TimedOut(limits.timeout));
            }
        }
    };
    // A child it left behind can hold the pipes open, its input included; it gets what is left of
    // the time limit, then the tree is ended so no reader or writer thread is left waiting.
    while !(stdout.finished() && stderr.finished() && writer_finished()) {
        if overran().is_some() || Instant::now() >= deadline {
            let cut_short = stdout.cut_short() || stderr.cut_short();
            tree.end();
            if overran().is_none() && cut_short {
                return Err(Unfinished::TimedOut(limits.timeout));
            }
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    if let Some(cap) = overran() {
        tree.end();
        return Err(Unfinished::TooLarge(cap));
    }
    Ok(RunOutput {
        status,
        stdout: stdout.bytes(),
        stderr: stderr.bytes(),
    })
}

/// One stream read to its end on a thread of its own, keeping what arrived so far readable.
struct Collected {
    keep: Keep,
    buffer: Arc<Mutex<Vec<u8>>>,
    overran: Arc<AtomicBool>,
    reader: JoinHandle<()>,
}

impl Collected {
    fn start(pipe: Option<impl Read + Send + 'static>, keep: Keep) -> Self {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let overran = Arc::new(AtomicBool::new(false));
        let (sink, overrun) = (buffer.clone(), overran.clone());
        let reader = std::thread::spawn(move || {
            let Some(mut pipe) = pipe else { return };
            let mut chunk = [0u8; 8192];
            while let Ok(read) = pipe.read(&mut chunk) {
                if read == 0 {
                    break;
                }
                let Ok(mut bytes) = sink.lock() else { break };
                match keep {
                    Keep::Head(cap) => {
                        if bytes.len() < cap {
                            bytes.extend_from_slice(&chunk[..read]);
                        }
                    }
                    Keep::Tail(cap) => {
                        bytes.extend_from_slice(&chunk[..read]);
                        let extra = bytes.len().saturating_sub(cap);
                        bytes.drain(..extra);
                    }
                    Keep::AtMost(cap) => {
                        if bytes.len() + read > cap {
                            overrun.store(true, Ordering::SeqCst);
                            break;
                        }
                        bytes.extend_from_slice(&chunk[..read]);
                    }
                }
            }
        });
        Self {
            keep,
            buffer,
            overran,
            reader,
        }
    }

    /// The cap it ran over, once it has.
    fn overran(&self) -> Option<usize> {
        match self.keep {
            Keep::AtMost(cap) if self.overran.load(Ordering::SeqCst) => Some(cap),
            _ => None,
        }
    }

    fn finished(&self) -> bool {
        self.reader.is_finished()
    }

    /// Whether it is kept whole (`Keep::AtMost`) and has not reached its end, so what it holds
    /// may be only the start of an answer.
    fn cut_short(&self) -> bool {
        matches!(self.keep, Keep::AtMost(_)) && !self.finished()
    }

    fn bytes(&self) -> Vec<u8> {
        self.buffer
            .lock()
            .map(|bytes| bytes.clone())
            .unwrap_or_default()
    }
}

#[cfg(all(test, unix))]
#[path = "process_run_tests.rs"]
mod tests;
