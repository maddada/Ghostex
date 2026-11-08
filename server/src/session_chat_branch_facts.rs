//! What each line of a Claude transcript says about the message tree, read once and kept, so the
//! branch boundary of an older page (`session_chat_branch::scan_session_chat_branch_boundary`) is
//! a walk over small records instead of a parse of every JSON line between the page and the end.
//!
//! CDXC:SessionChat 2026-11-08 WHY:
//! User: "scroll up a lot in this long chat we're in and then click on the Needs you question pill and it takes like 5 seconds before it jumps to it. Also pasting an image same takes ages." A page of an older part of a Claude chat needs the tree state the rows after it establish, and that pass parsed every line from the page to the nearest page read before it, or to the end of the file: on the Main Coordinator's 1 GB transcript a 50-row page 300 MB deep took 3.3 s in the user's gxserver, and only four passes were kept, so moving around the chat paid it again. Each line's lineage (its id, its parent, whether it is a prompt that can retract a branch, a leaf marker, a queue row) is now kept per transcript the first time a pass reads it, and every later pass over those bytes applies the kept records through the same rules (`BoundaryScan::apply_fact`). The records are dropped when the file is not the file they were read from (another inode, shrunk, or different bytes at either edge of what they cover), the same test the kept passes use.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::session_chat::{SessionChatLineDecoder, TranscriptFileVersion, TranscriptLeafMarker};

/// One line's part in the tree pass.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LineFact {
    /// Nothing the pass reads: not a lineage row.
    Skip,
    /// A queue bookkeeping row.
    Queue,
    /// An explicit `last-prompt` leaf marker.
    Marker(TranscriptLeafMarker),
    /// A tree row: its id, its parent, and whether it is a prompt that can retract a branch.
    Tree {
        id: Box<str>,
        parent: Option<Box<str>>,
        prompt: bool,
    },
}

/// How many bytes at each edge of the kept range are compared with the file before it is used.
const EDGE_BYTES: u64 = 64;

/// Transcripts whose records are kept, most recently used last.
const FILES_KEPT: usize = 2;

/// The records of one contiguous run of complete lines of one transcript.
#[derive(Clone, Debug, Default)]
pub(crate) struct LineFacts {
    path: PathBuf,
    identity: String,
    /// The decoder the prompt test ran with (a function address).
    decode: usize,
    /// The first line's offset and the end of the last line (after its newline).
    start: u64,
    end: u64,
    offsets: Vec<u64>,
    facts: Vec<LineFact>,
    head: Vec<u8>,
    tail: Vec<u8>,
}

fn kept() -> &'static Mutex<Vec<LineFacts>> {
    static KEPT: OnceLock<Mutex<Vec<LineFacts>>> = OnceLock::new();
    KEPT.get_or_init(|| Mutex::new(Vec::new()))
}

fn read_at(file: &mut File, at: u64, len: u64) -> Option<Vec<u8>> {
    let mut bytes = vec![0; usize::try_from(len).ok()?];
    file.seek(SeekFrom::Start(at)).ok()?;
    file.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

/// The records kept for this transcript, while they still describe it; taken out of the store
/// for the pass, which hands them back with [`keep`].
pub(crate) fn take(
    path: &Path,
    version: &TranscriptFileVersion,
    decode: SessionChatLineDecoder,
    file: &mut File,
) -> Option<LineFacts> {
    let mut kept = kept().lock().ok()?;
    let at = kept.iter().position(|facts| facts.path == path)?;
    let facts = kept.remove(at);
    drop(kept);
    let edges_hold = |file: &mut File| {
        read_at(file, facts.start, facts.head.len() as u64).as_deref() == Some(&facts.head[..])
            && read_at(
                file,
                facts.end - facts.tail.len() as u64,
                facts.tail.len() as u64,
            )
            .as_deref()
                == Some(&facts.tail[..])
    };
    (facts.identity == version.identity
        && facts.decode == decode as usize
        && facts.end <= version.size
        && edges_hold(file))
    .then_some(facts)
}

/// Puts the records back, with their edges read again from the file.
pub(crate) fn keep(mut facts: LineFacts, file: &mut File) {
    if facts.facts.is_empty() {
        return;
    }
    let span = facts.end - facts.start;
    let (Some(head), Some(tail)) = (
        read_at(file, facts.start, span.min(EDGE_BYTES)),
        read_at(file, facts.end - span.min(EDGE_BYTES), span.min(EDGE_BYTES)),
    ) else {
        return;
    };
    facts.head = head;
    facts.tail = tail;
    let Ok(mut kept) = kept().lock() else {
        return;
    };
    kept.retain(|known| known.path != facts.path);
    kept.push(facts);
    let excess = kept.len().saturating_sub(FILES_KEPT);
    kept.drain(..excess);
}

impl LineFacts {
    /// Empty records for this transcript, to be filled by a pass.
    pub(crate) fn new(
        path: &Path,
        version: &TranscriptFileVersion,
        decode: SessionChatLineDecoder,
    ) -> Self {
        Self {
            path: path.to_path_buf(),
            identity: version.identity.clone(),
            decode: decode as usize,
            ..Self::default()
        }
    }

    /// The kept lines from `offset` (a line start inside the kept run) up to `stop`, each with its
    /// offset, and where the last of them ends; `None` when `offset` is not a kept line.
    pub(crate) fn known_from(
        &self,
        offset: u64,
        stop: u64,
    ) -> Option<(impl Iterator<Item = (u64, &LineFact)>, u64)> {
        if self.facts.is_empty() || offset < self.start || offset >= self.end {
            return None;
        }
        let first = self.offsets.binary_search(&offset).ok()?;
        let last = self.offsets.partition_point(|at| *at < stop);
        if last <= first {
            return None;
        }
        let end = self.offsets.get(last).copied().unwrap_or(self.end);
        Some((
            self.offsets[first..last]
                .iter()
                .copied()
                .zip(self.facts[first..last].iter()),
            end,
        ))
    }

    /// The first kept line that starts after `offset`: where a pass reading from `offset` meets the
    /// kept records.
    pub(crate) fn next_start_after(&self, offset: u64) -> Option<u64> {
        let at = self.offsets.partition_point(|start| *start <= offset);
        self.offsets.get(at).copied()
    }

    /// Records one complete line read at `offset` that ends at `end`, after the kept run.
    pub(crate) fn push(&mut self, offset: u64, end: u64, fact: LineFact) {
        if self.facts.is_empty() {
            self.start = offset;
        }
        self.offsets.push(offset);
        self.facts.push(fact);
        self.end = end;
    }

    /// `self` (lines read just now) followed by `later`, the kept run that starts where they end.
    pub(crate) fn followed_by(mut self, later: LineFacts) -> LineFacts {
        if self.facts.is_empty() {
            return later;
        }
        if later.facts.is_empty() || self.end != later.start {
            return self;
        }
        self.offsets.extend(later.offsets);
        self.facts.extend(later.facts);
        self.end = later.end;
        self
    }

    pub(crate) fn start(&self) -> Option<u64> {
        (!self.facts.is_empty()).then_some(self.start)
    }

    pub(crate) fn end(&self) -> u64 {
        self.end
    }
}
