/*
CDXC:SessionChat 2026-09-02:
Active-branch selection for Claude transcripts. Claude Code's transcript is an
append-only message TREE (`uuid`/`parentUuid`), and a `/rewind` → "Restore
conversation" writes NOTHING when it happens: it truncates the agent's
in-memory conversation, and the abandoned rows only become identifiable once
the next prompt is appended with its `parentUuid` pointing back at the rewound
leaf. From then on the file carries two prompt children of one parent, and the
terminal shows only the newer one. Chat showed both.

Three rules decide what is on the active branch, in the order a reader needs
them:

  1. Prompt siblings. A real user prompt N attached to parent P retracts an
     OLDER real prompt child S of the same P, together with every row that
     descends from S. Only a real prompt can retract a branch (never a
     tool_result row, an attachment, a system row or an assistant row), so a
     turn's parallel tool calls and hook attachments, which give an ordinary
     parent many children, are untouched, and so are the compaction and
     resume boundaries that break the chain without ever producing two prompts
     on one parent. This subsumes the older no-descendants rule
     (CDXC:SessionChat 2026-08-18), whose subtree is empty by construction.
  2. Explicit leaf markers. Claude's own resume loader treats a
     `{"type":"last-prompt","leafUuid":…,"explicit":true}` row as the active
     leaf when nothing was written after it, so every tree row between that
     leaf and the marker is off-branch. A tree row appended after the marker is
     the new leaf and voids it. Non-explicit `last-prompt` rows are ordinary
     bookkeeping and are ignored.
  3. A pending rewind Ghostex drove itself (`session_chat_rewind_state`). The
     transcript is unchanged at that moment, so the store stands in for the
     marker rule 2 would read: it names the leaf and the transcript length at
     the time. The entry retires on the first real prompt written at or after
     that length, which either proves the rewind (it hangs off the leaf, so
     rule 1 takes over for good) or refutes it (the agent carried on from the
     old leaf and the rows must come back).

Rule 1 measured over the 120 most recent local transcripts: 2,065 of 55,622
rows dropped, in 12 of 120 files, every one a rewind or a revised re-send. The
noise-row exclusion in `transcript_message_is_branch_prompt` is load-bearing:
a released `<task-notification>` queue entry is written as a `user` row that
sometimes attaches to the session's ROOT row rather than to the leaf, and
counting it as a prompt retracted an entire 82-row session that the terminal
was still showing.

Claude only. Every other agent's lineage extractor is `None`, so every reader
here is a no-op for them.
*/
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::session_chat::{
    is_noise_message, parse_json_object, transcript_fallback_id, SessionChatLineDecoder,
    SessionChatMessage, SessionChatRole, TranscriptFileVersion, TranscriptLeafMarker,
    TranscriptLineage,
};
use crate::session_chat_decode_claude::{
    claude_record_type_can_be_prompt, claude_transcript_lineage_record,
};
use crate::session_chat_rewind_state::{
    clear_session_chat_pending_rewind, session_chat_pending_rewind,
};

/// The one classifier for "a row that can retract a branch". Every reader uses
/// it so a prompt means the same thing on the tail path, the append path and
/// the export.
pub(crate) fn transcript_message_is_branch_prompt(message: Option<&SessionChatMessage>) -> bool {
    let Some(message) = message else {
        return false;
    };
    message.role == SessionChatRole::User && !message.queued && !is_noise_message(message)
}

/// Cutoff installed by rule 2 or rule 3: every tree row older than it is
/// off-branch until `leaf` is reached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BranchCutoff {
    /// `None` means rewound to before the transcript's first message, so nothing
    /// older than the cutoff survives.
    pub(crate) leaf: Option<String>,
    /// Set only for a pending rewind (rule 3): rows at or after this offset
    /// were written AFTER the rewind and are on the new branch.
    pub(crate) pending_from: Option<u64>,
}

impl BranchCutoff {
    fn from_marker(marker: &TranscriptLeafMarker) -> Self {
        Self {
            leaf: marker.leaf_id().map(str::to_string),
            pending_from: None,
        }
    }
}

/// Everything a page of OLDER rows needs to prune exactly as the page above it
/// did. Computed for the boundary a page ended at, then either memoized or
/// rebuilt by `scan_session_chat_branch_boundary`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SessionChatBranchBoundary {
    /// Parents that a prompt ABOVE the boundary attaches to and whose own row
    /// is below it, so an older prompt child of theirs is retracted.
    pub(crate) prompt_parents: HashSet<String>,
    /// A cutoff still unreached at the boundary.
    pub(crate) cutoff: Option<BranchCutoff>,
}

impl SessionChatBranchBoundary {
    /// Boundary for a read that starts at the end of the file: nothing is above
    /// it, so only a pending rewind can be in force.
    fn at_end_of_file(file_path: &Path) -> Self {
        Self {
            prompt_parents: HashSet::new(),
            cutoff: session_chat_pending_rewind(file_path).map(|pending| BranchCutoff {
                leaf: pending.leaf_id,
                pending_from: Some(pending.cutoff_offset),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Backward scan (the tail reader's half)
// ---------------------------------------------------------------------------

/// What the tail reader must do with the row it just decoded.
pub(crate) enum BranchVerdict {
    Keep,
    /// Off-branch: do not emit it and do not count it against the page limit.
    Drop,
    /// Off-branch, and so is every row at these byte offsets, all of them
    /// already scanned, because a subtree is newer than its root.
    DropSubtree {
        offsets: Vec<u64>,
    },
}

/*
The scan runs newest-first, which is the wrong direction for both prompt
siblings (the dead subtree is reached BEFORE the prompt that proves it dead)
and leaf markers (the rows a marker kills are reached after it). Both are
handled by carrying "ids we still need to reach" and letting the reader keep
scanning past its page limit while any remain; a normal transcript resolves a
prompt's parent one row later, so the extra work only happens across a real
rewind.
*/
pub(crate) struct ActiveBranchScan {
    enabled: bool,
    file_path: PathBuf,
    /// Parents already known to carry a newer real prompt child.
    prompt_parents: HashSet<String>,
    /// Ids the scan still wants to reach: unresolved prompt parents plus a
    /// cutoff's leaf.
    unresolved: HashSet<String>,
    /// Only for rows scanned so far, so a retracted prompt's subtree can be
    /// walked downward from it.
    children: HashMap<String, Vec<String>>,
    row_offset: HashMap<String, u64>,
    /// `(prompt row offset, parent id)` for every real prompt scanned, so the
    /// boundary handed to the next page can be restricted to the rows that page
    /// will not see for itself.
    prompt_parent_rows: Vec<(u64, String)>,
    /// Tree rows the scan kept, newest first. Only its head is ever read: the
    /// active leaf, which the follower needs to tell an ordinary next prompt
    /// from one that re-attaches further up the tree.
    kept_tree_rows: Vec<(u64, String)>,
    /// Prompt parents inherited from the page above.
    inherited_prompt_parents: HashSet<String>,
    cutoff: Option<BranchCutoff>,
    cutoff_resolved_at: Option<u64>,
    tree_rows_seen: u64,
    pending_retired: bool,
    overscan_from: Option<u64>,
}

/*
Bound on how far the scan may run past its page limit to resolve a parent it has
not reached. Every prompt in a healthy transcript names the row right before it,
so this budget is only ever spent on a dead branch; a file whose newest prompt
names a parent that is not in the file at all (never observed across 120 local
transcripts, but a truncated or hand-edited file could) would otherwise re-read
the whole transcript on every follower snapshot.
*/
const BRANCH_RESOLUTION_OVERSCAN_BYTES: u64 = 16 * 1024 * 1024;

impl ActiveBranchScan {
    pub(crate) fn new(
        file_path: &Path,
        enabled: bool,
        boundary: SessionChatBranchBoundary,
    ) -> Self {
        let mut unresolved: HashSet<String> = boundary.prompt_parents.clone();
        if let Some(leaf) = boundary
            .cutoff
            .as_ref()
            .and_then(|cutoff| cutoff.leaf.clone())
        {
            unresolved.insert(leaf);
        }
        Self {
            enabled,
            file_path: file_path.to_path_buf(),
            prompt_parents: boundary.prompt_parents.clone(),
            unresolved,
            children: HashMap::new(),
            row_offset: HashMap::new(),
            prompt_parent_rows: Vec::new(),
            kept_tree_rows: Vec::new(),
            inherited_prompt_parents: boundary.prompt_parents,
            cutoff: boundary.cutoff,
            cutoff_resolved_at: None,
            tree_rows_seen: 0,
            pending_retired: false,
            overscan_from: None,
        }
    }

    /// `true` while the scan still has an id to reach. Checked once per line, so
    /// the overscan budget is measured from the offset the page limit was first
    /// satisfied at.
    pub(crate) fn keep_scanning(&mut self, offset: u64) -> bool {
        if !self.enabled || self.unresolved.is_empty() {
            return false;
        }
        let started_at = *self.overscan_from.get_or_insert(offset);
        started_at.saturating_sub(offset) <= BRANCH_RESOLUTION_OVERSCAN_BYTES
    }

    pub(crate) fn observe(
        &mut self,
        offset: u64,
        lineage: &TranscriptLineage,
        message: Option<&SessionChatMessage>,
    ) -> BranchVerdict {
        if !self.enabled {
            return BranchVerdict::Keep;
        }
        if let Some(marker) = lineage.leaf_marker.as_ref() {
            // Newest-first: a marker is only in force when nothing in the tree
            // was written after it.
            if self.tree_rows_seen == 0 && self.cutoff.is_none() {
                if let Some(leaf) = marker.leaf_id() {
                    self.unresolved.insert(leaf.to_string());
                }
                self.cutoff = Some(BranchCutoff::from_marker(marker));
            }
            return BranchVerdict::Keep;
        }
        if lineage.queue.is_some() {
            // Queue bookkeeping carries no tree position at all.
            return BranchVerdict::Keep;
        }
        self.tree_rows_seen += 1;
        self.row_offset.insert(lineage.id.clone(), offset);
        self.unresolved.remove(&lineage.id);
        if let Some(parent_id) = lineage.parent_id.as_ref() {
            self.children
                .entry(parent_id.clone())
                .or_default()
                .push(lineage.id.clone());
        }
        let is_prompt = transcript_message_is_branch_prompt(message);
        if is_prompt {
            self.retire_pending_rewind(offset, lineage);
        }
        if let Some(verdict) = self.cutoff_verdict(offset, lineage) {
            return verdict;
        }
        if is_prompt {
            if let Some(parent_id) = lineage.parent_id.clone() {
                self.prompt_parent_rows.push((offset, parent_id.clone()));
                if self.prompt_parents.contains(&parent_id) {
                    let offsets = self.subtree_offsets(&lineage.id);
                    let dropped: HashSet<u64> = offsets.iter().copied().collect();
                    self.kept_tree_rows
                        .retain(|(offset, _)| !dropped.contains(offset));
                    return BranchVerdict::DropSubtree { offsets };
                }
                if !self.row_offset.contains_key(&parent_id) {
                    self.unresolved.insert(parent_id.clone());
                }
                self.prompt_parents.insert(parent_id);
            }
        }
        self.kept_tree_rows.push((offset, lineage.id.clone()));
        BranchVerdict::Keep
    }

    /// Id of the newest row that survived, which is the row an ordinary next
    /// prompt will name as its parent.
    pub(crate) fn newest_kept_row_id(&self) -> Option<String> {
        self.kept_tree_rows
            .first()
            .map(|(_, id)| id.clone())
            .filter(|_| self.enabled)
    }

    /*
    Rule 3's retirement. The first real prompt written at or after the recorded
    transcript length answers whether the rewind took: a prompt hanging off the
    recorded leaf (or, for a rewind to before the first message, a prompt with
    no parent at all) proves it, and anything else refutes it. Either way the
    store entry is done. Once proven, rule 1 has the persisted signature it
    needs and never has to ask again.
    */
    fn retire_pending_rewind(&mut self, offset: u64, lineage: &TranscriptLineage) {
        if self.pending_retired {
            return;
        }
        let Some(cutoff) = self.cutoff.as_ref() else {
            return;
        };
        let Some(pending_from) = cutoff.pending_from else {
            return;
        };
        if offset < pending_from {
            return;
        }
        self.pending_retired = true;
        let proven = lineage.parent_id.as_deref() == cutoff.leaf.as_deref();
        clear_session_chat_pending_rewind(&self.file_path);
        if !proven {
            self.cutoff = None;
        }
    }

    /// Rules 2 and 3 share the same cutoff shape: below it, every tree row is
    /// off-branch until the leaf row itself is reached.
    fn cutoff_verdict(
        &mut self,
        offset: u64,
        lineage: &TranscriptLineage,
    ) -> Option<BranchVerdict> {
        let cutoff = self.cutoff.as_ref()?;
        if self.cutoff_resolved_at.is_some() {
            return None;
        }
        if cutoff
            .pending_from
            .is_some_and(|pending_from| offset >= pending_from)
        {
            return None;
        }
        if cutoff.leaf.as_deref() == Some(lineage.id.as_str()) {
            // The leaf row itself is the newest ON-branch row.
            self.cutoff_resolved_at = Some(offset);
            return None;
        }
        Some(BranchVerdict::Drop)
    }

    /// Byte offsets of `root` and every scanned row under it. A subtree is
    /// always newer than its root, so a newest-first scan has already seen all
    /// of it by the time the root is retracted.
    fn subtree_offsets(&self, root: &str) -> Vec<u64> {
        let mut offsets = Vec::new();
        let mut queue: VecDeque<&str> = VecDeque::new();
        let mut visited: HashSet<&str> = HashSet::new();
        queue.push_back(root);
        visited.insert(root);
        while let Some(id) = queue.pop_front() {
            if let Some(offset) = self.row_offset.get(id) {
                offsets.push(*offset);
            }
            let Some(children) = self.children.get(id) else {
                continue;
            };
            for child in children {
                if visited.insert(child.as_str()) {
                    queue.push_back(child.as_str());
                }
            }
        }
        offsets
    }

    /// State the next page must start from, restricted to what it cannot work
    /// out for itself: parents named by prompts above `before_offset` whose own
    /// row is below it, plus a cutoff that was still unreached there.
    pub(crate) fn boundary_at(&self, before_offset: u64) -> SessionChatBranchBoundary {
        if !self.enabled {
            return SessionChatBranchBoundary::default();
        }
        let mut prompt_parents = self.inherited_prompt_parents.clone();
        for (offset, parent_id) in &self.prompt_parent_rows {
            if *offset >= before_offset {
                prompt_parents.insert(parent_id.clone());
            }
        }
        prompt_parents.retain(|parent_id| {
            self.row_offset
                .get(parent_id)
                .is_none_or(|offset| *offset < before_offset)
        });
        let cutoff = self
            .cutoff
            .clone()
            .filter(|_| self.cutoff_resolved_at.is_none_or(|at| at < before_offset));
        SessionChatBranchBoundary {
            prompt_parents,
            cutoff,
        }
    }
}

// ---------------------------------------------------------------------------
// Boundary state for a paginated read
// ---------------------------------------------------------------------------

/*
Pagination hands the client one opaque `beforeOffset` and asks for the rows
below it, so a page that starts inside (or above) a dead subtree has to be
told what the page above it already knew. The memo is filled by the page that
established the boundary and read by the page that continues from it, keyed by
the file's identity AND size because an append can add a prompt whose parent
sits below the boundary. A miss (a restart, a client resuming an old cursor, or
any write since the last page) is answered by re-deriving the state from the
rows above the boundary, which is exact and needs no cache at all.
*/
const BRANCH_BOUNDARY_MEMO_LIMIT: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct BranchBoundaryKey {
    path: PathBuf,
    identity: String,
    size: u64,
    offset: u64,
}

type BranchBoundaryMemo = (
    VecDeque<BranchBoundaryKey>,
    HashMap<BranchBoundaryKey, SessionChatBranchBoundary>,
);

fn branch_boundary_memo() -> &'static Mutex<BranchBoundaryMemo> {
    static MEMO: OnceLock<Mutex<BranchBoundaryMemo>> = OnceLock::new();
    MEMO.get_or_init(|| Mutex::new((VecDeque::new(), HashMap::new())))
}

fn branch_boundary_key(
    file_path: &Path,
    version: &TranscriptFileVersion,
    offset: u64,
) -> BranchBoundaryKey {
    BranchBoundaryKey {
        path: file_path.to_path_buf(),
        identity: version.identity.clone(),
        size: version.size,
        offset,
    }
}

pub(crate) fn remember_session_chat_branch_boundary(
    file_path: &Path,
    version: &TranscriptFileVersion,
    offset: u64,
    boundary: SessionChatBranchBoundary,
) {
    let key = branch_boundary_key(file_path, version, offset);
    let Ok(mut memo) = branch_boundary_memo().lock() else {
        return;
    };
    let (order, entries) = &mut *memo;
    if entries.insert(key.clone(), boundary).is_none() {
        order.push_back(key);
    }
    while order.len() > BRANCH_BOUNDARY_MEMO_LIMIT {
        if let Some(evicted) = order.pop_front() {
            entries.remove(&evicted);
        }
    }
}

/// Boundary state for a read whose window ends at `end_offset`. A `version` of
/// `None` is an agent whose transcript is not a tree, and an `end_offset` at or
/// past the end of the file is the live tail, which has nothing above it.
pub(crate) fn session_chat_branch_boundary(
    file_path: &Path,
    version: Option<&TranscriptFileVersion>,
    end_offset: Option<u64>,
    decode: SessionChatLineDecoder,
) -> SessionChatBranchBoundary {
    let Some(version) = version else {
        return SessionChatBranchBoundary::default();
    };
    let Some(end_offset) = end_offset.filter(|end| *end < version.size) else {
        return SessionChatBranchBoundary::at_end_of_file(file_path);
    };
    let key = branch_boundary_key(file_path, version, end_offset);
    if let Ok(memo) = branch_boundary_memo().lock() {
        if let Some(boundary) = memo.1.get(&key) {
            return boundary.clone();
        }
    }
    scan_session_chat_branch_boundary(file_path, version, end_offset, decode).unwrap_or_default()
}

/*
Forward re-derivation of the state at `end_offset` from the rows above it. Only
rows that can carry a prompt are decoded; every other row is read for its
`uuid`/`parentUuid` alone, because all the pass needs from them is whether they
resolve an id the page below is still waiting for.

CDXC:SessionChat 2026-11-05 WHY:
The memo above is keyed by the file's size, so for a chat that is still being
written every request for an older page missed it and read the transcript from
that page to its end again: 71% of gxserver's CPU while a client paged a long,
active chat (measured 2026-11-05, issues-1027/macos-perf). The pass only moves
forward, so its state at the last complete line is kept (`BoundaryScan`) and the
next request for the same page reads only what was written since. A kept state
is used only while the file is the same file (identity, no shrink, the bytes
before where it stopped unchanged) and the pending rewind it started from is
unchanged; otherwise the pass starts over. A last line without its newline yet
is read into a copy, never into the kept state, exactly as a pass from the start
would read it.

CDXC:SessionChat 2026-11-07 WHY:
That kept state only served a page asked for again. The NEXT older page has a
new boundary, so it still read from itself to the end of the file: on the main
coordinator's 890 MB transcript every page a client loaded while scrolling up
(or jumping to an old question) re-parsed everything newer than it, about
1.3 s of a full core per page 800 MB deep, which kept gxserver near one core
while the user scrolled (measured 2026-11-07, backlog-1107). A pass over the
rows above a boundary is the pass over the rows down to the next kept boundary
followed by that boundary's own pass, so a new boundary now reads only the rows
between itself and the nearest kept one above it (`followed_by`), and takes
over that kept state instead of copying it.
*/
#[derive(Clone)]
struct BoundaryScan {
    path: PathBuf,
    identity: String,
    end_offset: u64,
    pending_input: Option<crate::session_chat_rewind_state::SessionChatPendingRewind>,
    /// Bytes consumed, always at the end of a complete line.
    scanned_to: u64,
    /// The last bytes before `scanned_to` and the first ones from `end_offset`, to tell an
    /// append from a rewrite (transcripts are only appended to; a rewrite that leaves both alone
    /// and keeps the size is not seen).
    tail: Vec<u8>,
    head: Vec<u8>,
    ids_seen: HashSet<String>,
    prompt_parents: HashSet<String>,
    marker_in_force: Option<TranscriptLeafMarker>,
    pending: Option<BranchCutoff>,
    pending_retired: bool,
}

/// Scans kept, most recently used last; each holds the ids above one page, so only a few. Tests
/// share the list across threads, so they keep more and never lose a scan to another test.
const BOUNDARY_SCANS_KEPT: usize = if cfg!(test) { 256 } else { 4 };
const BOUNDARY_SCAN_TAIL_BYTES: usize = 64;

fn boundary_scans() -> &'static Mutex<Vec<BoundaryScan>> {
    static SCANS: OnceLock<Mutex<Vec<BoundaryScan>>> = OnceLock::new();
    SCANS.get_or_init(|| Mutex::new(Vec::new()))
}

impl BoundaryScan {
    fn new(
        path: &Path,
        identity: &str,
        end_offset: u64,
        pending_input: Option<crate::session_chat_rewind_state::SessionChatPendingRewind>,
    ) -> Self {
        let pending = pending_input.as_ref().map(|pending| BranchCutoff {
            leaf: pending.leaf_id.clone(),
            pending_from: Some(pending.cutoff_offset),
        });
        Self {
            path: path.to_path_buf(),
            identity: identity.to_string(),
            end_offset,
            pending_input,
            scanned_to: end_offset,
            tail: Vec::new(),
            head: Vec::new(),
            ids_seen: HashSet::new(),
            prompt_parents: HashSet::new(),
            marker_in_force: None,
            pending,
            pending_retired: false,
        }
    }

    /// One line of the transcript (without its line ending) that starts at `line_offset`.
    fn apply(
        &mut self,
        file_path: &Path,
        line_offset: u64,
        mut bytes: &[u8],
        decode: SessionChatLineDecoder,
    ) {
        if bytes.last() == Some(&b'\r') {
            bytes = &bytes[..bytes.len() - 1];
        }
        if bytes.is_empty() {
            return;
        }
        let line = String::from_utf8_lossy(bytes).into_owned();
        let Some(record) = parse_json_object(&line) else {
            return;
        };
        let fallback_id = transcript_fallback_id(file_path, line_offset);
        let Some(row) = claude_transcript_lineage_record(&record, &fallback_id) else {
            return;
        };
        if let Some(marker) = row.leaf_marker {
            self.marker_in_force = Some(marker);
            return;
        }
        if row.queue.is_some() {
            return;
        }
        self.marker_in_force = None;
        self.ids_seen.insert(row.id.clone());
        if !claude_record_type_can_be_prompt(&record) {
            return;
        }
        if !transcript_message_is_branch_prompt(decode(&line, &fallback_id).as_ref()) {
            return;
        }
        if let Some(parent_id) = row.parent_id.clone() {
            self.prompt_parents.insert(parent_id);
        }
        if let Some(cutoff) = self.pending.as_ref() {
            if !self.pending_retired && cutoff.pending_from.is_some_and(|from| line_offset >= from)
            {
                self.pending_retired = true;
                if row.parent_id.as_deref() != cutoff.leaf.as_deref() {
                    self.pending = None;
                }
            }
        }
    }

    fn boundary(&self) -> SessionChatBranchBoundary {
        // A parent whose own row is above the boundary was already resolved there,
        // and a cutoff whose leaf is above it has nothing left to retract below.
        let mut prompt_parents = self.prompt_parents.clone();
        prompt_parents.retain(|parent_id| !self.ids_seen.contains(parent_id));
        let cutoff = self
            .marker_in_force
            .as_ref()
            .map(BranchCutoff::from_marker)
            .or(self.pending.clone())
            .filter(|cutoff| {
                cutoff
                    .leaf
                    .as_ref()
                    .is_none_or(|leaf| !self.ids_seen.contains(leaf))
            });
        SessionChatBranchBoundary {
            prompt_parents,
            cutoff,
        }
    }

    /// This pass over the rows from `end_offset` to `later.end_offset`, followed by `later`, the
    /// pass from there on: the state a single pass from `end_offset` reaches.
    fn followed_by(self, later: BoundaryScan) -> BoundaryScan {
        // Only a tree row clears a marker and every tree row lands in `ids_seen`, so a later pass
        // that saw neither a marker nor a tree row leaves this pass's marker in force.
        let marker_in_force = if later.marker_in_force.is_some() || !later.ids_seen.is_empty() {
            later.marker_in_force
        } else {
            self.marker_in_force
        };
        // The first prompt written after the rewind retires it, and this pass's rows come first.
        let (pending, pending_retired) = if self.pending_retired {
            (self.pending, true)
        } else {
            (later.pending, later.pending_retired)
        };
        let mut ids_seen = later.ids_seen;
        ids_seen.extend(self.ids_seen);
        let mut prompt_parents = later.prompt_parents;
        prompt_parents.extend(self.prompt_parents);
        let mut head = self.head;
        let room = BOUNDARY_SCAN_TAIL_BYTES.saturating_sub(head.len());
        head.extend_from_slice(&later.head[..later.head.len().min(room)]);
        BoundaryScan {
            path: later.path,
            identity: later.identity,
            end_offset: self.end_offset,
            pending_input: later.pending_input,
            scanned_to: later.scanned_to,
            tail: later.tail,
            head,
            ids_seen,
            prompt_parents,
            marker_in_force,
            pending,
            pending_retired,
        }
    }

    /// Whether this kept scan can continue for `version` of the file it was made from.
    fn continues(&self, file: &mut std::fs::File, version: &TranscriptFileVersion) -> bool {
        if self.identity != version.identity || self.scanned_to > version.size {
            return false;
        }
        let same = |file: &mut std::fs::File, at: u64, kept: &[u8]| {
            let mut bytes = vec![0; kept.len()];
            kept.is_empty()
                || (file.seek(SeekFrom::Start(at)).is_ok()
                    && std::io::Read::read_exact(file, &mut bytes).is_ok()
                    && bytes == kept)
        };
        same(file, self.end_offset, &self.head)
            && same(file, self.scanned_to - self.tail.len() as u64, &self.tail)
    }
}

fn scan_session_chat_branch_boundary(
    file_path: &Path,
    version: &TranscriptFileVersion,
    end_offset: u64,
    decode: SessionChatLineDecoder,
) -> std::io::Result<SessionChatBranchBoundary> {
    let mut file = std::fs::File::open(file_path)?;
    let pending_input = session_chat_pending_rewind(file_path);
    // The kept pass for this boundary, else the one for the nearest boundary above it.
    let kept = boundary_scans().lock().ok().and_then(|mut scans| {
        let at = scans
            .iter()
            .enumerate()
            .filter(|(_, scan)| {
                scan.path == file_path
                    && scan.end_offset >= end_offset
                    && scan.pending_input == pending_input
            })
            .min_by_key(|(_, scan)| scan.end_offset)?
            .0;
        Some(scans.remove(at))
    });
    let new_scan = || {
        BoundaryScan::new(
            file_path,
            &version.identity,
            end_offset,
            pending_input.clone(),
        )
    };
    let mut resumed = None;
    if let Some(mut above) = kept.filter(|scan| scan.continues(&mut file, version)) {
        let unfinished = scan_forward(&file, file_path, &mut above, None, decode)?;
        if above.end_offset == end_offset {
            resumed = Some((above, unfinished));
        } else {
            let mut rows_between = new_scan();
            let cut = scan_forward(
                &file,
                file_path,
                &mut rows_between,
                Some(above.end_offset),
                decode,
            )?;
            // A kept boundary that is not on a line of this file is dropped and the pass starts over.
            if cut.is_none() && rows_between.scanned_to == above.end_offset {
                resumed = Some((rows_between.followed_by(above), unfinished));
            }
        }
    }
    let (scan, unfinished) = match resumed {
        Some(resumed) => resumed,
        None => {
            let mut scan = new_scan();
            let unfinished = scan_forward(&file, file_path, &mut scan, None, decode)?;
            (scan, unfinished)
        }
    };
    let boundary = match unfinished {
        // A last line still being written: read into a copy, as a pass from the start would.
        Some((line_offset, bytes)) => {
            let mut copy = scan.clone();
            copy.apply(file_path, line_offset, &bytes, decode);
            copy.boundary()
        }
        None => scan.boundary(),
    };
    if let Ok(mut scans) = boundary_scans().lock() {
        scans.push(scan);
        let excess = scans.len().saturating_sub(BOUNDARY_SCANS_KEPT);
        scans.drain(..excess);
    }
    Ok(boundary)
}

/// Applies every complete line from where `scan` stopped up to `until` (the end of the file when
/// `None`), and returns a last line that has no newline yet with its offset.
fn scan_forward(
    mut file: &std::fs::File,
    file_path: &Path,
    scan: &mut BoundaryScan,
    until: Option<u64>,
    decode: SessionChatLineDecoder,
) -> std::io::Result<Option<(u64, Vec<u8>)>> {
    file.seek(SeekFrom::Start(scan.scanned_to))?;
    let limit = until.map_or(u64::MAX, |until| until.saturating_sub(scan.scanned_to));
    let mut reader = BufReader::new(std::io::Read::take(file, limit));
    let mut offset = scan.scanned_to;
    loop {
        let line_offset = offset;
        let mut bytes = Vec::new();
        let read = reader.read_until(b'\n', &mut bytes)?;
        if read == 0 {
            return Ok(None);
        }
        offset += read as u64;
        if bytes.last() != Some(&b'\n') {
            return Ok(Some((line_offset, bytes)));
        }
        bytes.pop();
        scan.apply(file_path, line_offset, &bytes, decode);
        scan.scanned_to = offset;
        bytes.push(b'\n');
        scan.tail.extend_from_slice(&bytes);
        let excess = scan.tail.len().saturating_sub(BOUNDARY_SCAN_TAIL_BYTES);
        scan.tail.drain(..excess);
        if scan.head.len() < BOUNDARY_SCAN_TAIL_BYTES {
            let room = BOUNDARY_SCAN_TAIL_BYTES - scan.head.len();
            scan.head.extend_from_slice(&bytes[..bytes.len().min(room)]);
        }
    }
}

// ---------------------------------------------------------------------------
// Forward pass (the transcript export's half)
// ---------------------------------------------------------------------------

struct ForwardBranchRow {
    index: usize,
    id: String,
    parent_id: Option<String>,
    is_prompt: bool,
}

/*
The same three rules for a reader that already holds every line in order: the
export. Forward is the natural direction for rule 1 (the retracted sibling is
behind us, so its subtree is walked at the end) and the awkward one for rule 2
(a marker is only in force if it is the last thing in the file).
*/
pub(crate) fn claude_off_branch_line_indices(
    file_path: &Path,
    lines: &[String],
    decode: SessionChatLineDecoder,
) -> HashSet<usize> {
    let mut rows: Vec<ForwardBranchRow> = Vec::new();
    let mut index_of_id: HashMap<String, usize> = HashMap::new();
    let mut marker_in_force: Option<TranscriptLeafMarker> = None;
    let mut offset = 0u64;
    // Byte offsets are reconstructed for rule 3 alone, from the line lengths the
    // export itself read.
    let mut offset_of_row: Vec<u64> = Vec::new();
    let mut pending = session_chat_pending_rewind(file_path);
    let mut pending_retired = false;
    for (index, line) in lines.iter().enumerate() {
        let line_offset = offset;
        offset += line.len() as u64 + 1;
        let Some(record) = parse_json_object(line) else {
            continue;
        };
        let fallback_id = transcript_fallback_id(file_path, line_offset);
        let Some(row) = claude_transcript_lineage_record(&record, &fallback_id) else {
            continue;
        };
        if let Some(marker) = row.leaf_marker {
            marker_in_force = Some(marker);
            continue;
        }
        if row.queue.is_some() {
            continue;
        }
        marker_in_force = None;
        let is_prompt = claude_record_type_can_be_prompt(&record)
            && transcript_message_is_branch_prompt(decode(line, &fallback_id).as_ref());
        // Rule 3's retirement, read-only: a prompt written after the rewind
        // that does not hang off the recorded leaf refutes it. Clearing the
        // store entry is the live readers' job, never an export's.
        if is_prompt && !pending_retired {
            if let Some(entry) = pending.as_ref() {
                if line_offset >= entry.cutoff_offset {
                    pending_retired = true;
                    if row.parent_id.as_deref() != entry.leaf_id.as_deref() {
                        pending = None;
                    }
                }
            }
        }
        index_of_id.insert(row.id.clone(), rows.len());
        offset_of_row.push(line_offset);
        rows.push(ForwardBranchRow {
            index,
            id: row.id,
            parent_id: row.parent_id,
            is_prompt,
        });
    }

    let mut dead_roots: Vec<usize> = Vec::new();
    let mut newest_prompt_child: HashMap<String, usize> = HashMap::new();
    for (position, row) in rows.iter().enumerate() {
        let (true, Some(parent_id)) = (row.is_prompt, row.parent_id.as_ref()) else {
            continue;
        };
        if let Some(previous) = newest_prompt_child.insert(parent_id.clone(), position) {
            dead_roots.push(previous);
        }
    }
    if let Some(marker) = marker_in_force.as_ref() {
        dead_roots.extend(rows_after_leaf(rows.len(), &index_of_id, marker.leaf_id()));
    }
    if let Some(entry) = pending.as_ref() {
        // Rule 3 kills the same span as a marker would, bounded to the rows
        // that existed when the rewind was accepted.
        dead_roots.extend(
            rows_after_leaf(rows.len(), &index_of_id, entry.leaf_id.as_deref())
                .into_iter()
                .filter(|position| offset_of_row[*position] < entry.cutoff_offset),
        );
    }

    let mut children: HashMap<&str, Vec<usize>> = HashMap::new();
    for (position, row) in rows.iter().enumerate() {
        if let Some(parent_id) = row.parent_id.as_deref() {
            children.entry(parent_id).or_default().push(position);
        }
    }
    let mut dead: HashSet<usize> = HashSet::new();
    let mut queue: VecDeque<usize> = dead_roots.into_iter().collect();
    while let Some(position) = queue.pop_front() {
        if !dead.insert(position) {
            continue;
        }
        let Some(children) = children.get(rows[position].id.as_str()) else {
            continue;
        };
        for child in children {
            queue.push_back(*child);
        }
    }
    dead.into_iter()
        .map(|position| rows[position].index)
        .collect()
}

/// Positions of every tree row written after `leaf` (all of them when the
/// conversation was rewound to before its first message).
fn rows_after_leaf(
    row_count: usize,
    index_of_id: &HashMap<String, usize>,
    leaf: Option<&str>,
) -> Vec<usize> {
    let start = match leaf {
        Some(leaf) => match index_of_id.get(leaf) {
            Some(position) => position + 1,
            // The leaf is not in this file, so nothing here is below it.
            None => return Vec::new(),
        },
        None => 0,
    };
    (start..row_count).collect()
}

#[cfg(test)]
mod boundary_scan_tests {
    use super::*;
    use crate::session_chat_decode_claude::decode_claude_transcript_line;
    use std::io::Write;

    fn prompt(id: &str, parent: Option<&str>, text: &str) -> String {
        serde_json::json!({
            "type": "user", "uuid": id, "parentUuid": parent,
            "timestamp": "2026-11-05T10:00:00.000Z",
            "message": { "role": "user", "content": text },
        })
        .to_string()
    }

    fn reply(id: &str, parent: &str) -> String {
        serde_json::json!({
            "type": "assistant", "uuid": id, "parentUuid": parent,
            "timestamp": "2026-11-05T10:00:01.000Z",
            "message": { "role": "assistant", "content": [{ "type": "text", "text": "ok" }] },
        })
        .to_string()
    }

    fn marker(leaf: &str) -> String {
        serde_json::json!({ "type": "last-prompt", "leafUuid": leaf }).to_string()
    }

    /// What a pass from the start gives, with nothing kept.
    fn fresh(path: &Path, end_offset: u64) -> SessionChatBranchBoundary {
        boundary_scans()
            .lock()
            .unwrap()
            .retain(|scan| scan.path != path);
        let version = crate::session_chat::read_transcript_file_version(path).unwrap();
        scan_session_chat_branch_boundary(path, &version, end_offset, decode_claude_transcript_line)
            .unwrap()
    }

    fn resumed(path: &Path, end_offset: u64) -> SessionChatBranchBoundary {
        let version = crate::session_chat::read_transcript_file_version(path).unwrap();
        scan_session_chat_branch_boundary(path, &version, end_offset, decode_claude_transcript_line)
            .unwrap()
    }

    fn append(path: &Path, text: &str) {
        std::fs::OpenOptions::new()
            .append(true)
            .open(path)
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
    }

    /// A page's boundary read again after the transcript grew continues from where the last read
    /// stopped and gives what a read from the page to the end gives, through unfinished last lines,
    /// rewinds and a rewrite of the file in place.
    #[test]
    fn a_resumed_scan_matches_a_scan_from_the_start() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let head = [prompt("p1", None, "one"), reply("a1", "p1")].join("\n") + "\n";
        std::fs::write(&path, &head).unwrap();
        let end_offset = head.len() as u64;
        append(&path, &(reply("a1b", "a1") + "\n"));
        let check = |path: &Path, step: &str| {
            let kept = resumed(path, end_offset);
            let from_start = fresh(path, end_offset);
            assert_eq!(kept, from_start, "{step}");
            // Keep a scan for the next step, as a client paging this chat would.
            resumed(path, end_offset);
        };
        check(&path, "first read");
        // A new prompt under a row above the page, then its reply.
        append(
            &path,
            &(prompt("p2", Some("a1"), "two") + "\n" + &reply("a2", "p2") + "\n"),
        );
        check(&path, "appended complete lines");
        // A line still being written, then the rest of it.
        let late = prompt("p3", Some("a1b"), "three");
        let (first, rest) = late.split_at(late.len() / 2);
        append(&path, first);
        check(&path, "half a line");
        append(&path, &(rest.to_string() + "\n"));
        check(&path, "the line finished");
        // A rewind marker, then a prompt that voids it.
        append(&path, &(marker("a1") + "\n"));
        check(&path, "a leaf marker");
        append(&path, &(prompt("p4", Some("a1"), "four") + "\n"));
        check(&path, "a prompt after the marker");
        // The same file rewritten in place with other rows of at least the same length.
        let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        let rewritten = [
            prompt("q1", None, "other"),
            reply("b1", "q1"),
            prompt("q2", Some("zz"), "x".repeat(400).as_str()),
        ]
        .join("\n")
            + "\n";
        file.write_all(rewritten.as_bytes()).unwrap();
        drop(file);
        check(&path, "rewritten in place");
    }

    /// An older page's boundary taken from the kept pass of a newer page's boundary, plus the
    /// rows between the two, matches a pass from the start: for every pair of boundaries, with
    /// rows appended in between, through leaf markers, a retracted prompt and a pending rewind.
    #[test]
    fn a_boundary_derived_from_a_newer_one_matches_a_scan_from_the_start() {
        let explicit_marker = |leaf: &str| {
            serde_json::json!({ "type": "last-prompt", "leafUuid": leaf, "explicit": true })
                .to_string()
        };
        let lines = [
            prompt("p1", None, "one"),
            reply("a1", "p1"),
            prompt("p2", Some("a1"), "two"),
            reply("a2", "p2"),
            explicit_marker("a1"),
            reply("a2b", "a2"),
            explicit_marker("a2"),
            prompt("p3", Some("a1"), "three, retracting two"),
            reply("a3", "p3"),
            prompt("p4", Some("a3"), "four"),
            reply("a4", "p4"),
            explicit_marker("a3"),
        ];
        let appended = [
            prompt("p5", Some("a3"), "five, after the marker"),
            reply("a5", "p5"),
            prompt("p6", Some("a1"), "six"),
        ];
        let starts: Vec<u64> = lines
            .iter()
            .scan(0u64, |offset, line| {
                let start = *offset;
                *offset += line.len() as u64 + 1;
                Some(start)
            })
            .collect();
        let dir = tempfile::tempdir().unwrap();
        for pending in [false, true] {
            for newer in 1..lines.len() {
                for older in 0..newer {
                    let path = dir
                        .path()
                        .join(format!("t-{pending}-{older}-{newer}.jsonl"));
                    std::fs::write(&path, lines.join("\n") + "\n").unwrap();
                    if pending {
                        crate::session_chat_rewind_state::set_session_chat_pending_rewind(
                            &path,
                            crate::session_chat_rewind_state::SessionChatPendingRewind {
                                leaf_id: Some("a1".to_string()),
                                cutoff_offset: starts[5],
                                set_at_ms: 0,
                            },
                        );
                    }
                    let step = format!("pending {pending}, older {older}, newer {newer}");
                    // The newer page's pass is kept, then the chat grows before the older page.
                    fresh(&path, starts[newer]);
                    append(&path, &(appended.join("\n") + "\n"));
                    let derived = resumed(&path, starts[older]);
                    assert!(
                        boundary_scans()
                            .lock()
                            .unwrap()
                            .iter()
                            .any(|scan| scan.path == path && scan.end_offset == starts[older]),
                        "{step}: the older boundary's pass is kept"
                    );
                    assert_eq!(derived, fresh(&path, starts[older]), "{step}");
                    boundary_scans()
                        .lock()
                        .unwrap()
                        .retain(|scan| scan.path != path);
                }
            }
        }
        // The older page really starts from the newer page's kept pass: a parent planted in that
        // pass shows up in the older boundary.
        let path = dir.path().join("planted.jsonl");
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        fresh(&path, starts[8]);
        for scan in boundary_scans().lock().unwrap().iter_mut() {
            if scan.path == path {
                scan.prompt_parents.insert("planted".to_string());
            }
        }
        assert!(resumed(&path, starts[3]).prompt_parents.contains("planted"));
    }

    /// A client paging back through a long Claude chat that is still being written: one row is
    /// appended before every page, as a working coordinator does. Measures the history reads
    /// alone. Run on a disposable copy of a real transcript (rows are appended to it, then cut
    /// off again):
    /// `GX_BRANCH_BENCH_TRANSCRIPT=<copy.jsonl> [GX_BRANCH_BENCH_PAGES=20]
    /// [GX_BRANCH_BENCH_START=0.5] cargo test --lib paging_back_through_a_growing_transcript
    /// -- --ignored --nocapture`
    #[test]
    #[ignore = "benchmark: needs GX_BRANCH_BENCH_TRANSCRIPT, a disposable copy of a long Claude transcript"]
    fn paging_back_through_a_growing_transcript() {
        let path = PathBuf::from(std::env::var("GX_BRANCH_BENCH_TRANSCRIPT").unwrap());
        let pages: usize = std::env::var("GX_BRANCH_BENCH_PAGES")
            .ok()
            .and_then(|pages| pages.parse().ok())
            .unwrap_or(20);
        let start: f64 = std::env::var("GX_BRANCH_BENCH_START")
            .ok()
            .and_then(|start| start.parse().ok())
            .unwrap_or(1.0);
        let original_size = std::fs::metadata(&path).unwrap().len();
        // The first page starts on a line, `start` of the way into the file.
        let mut cursor = {
            let at = (original_size as f64 * start) as u64;
            if at >= original_size {
                original_size
            } else {
                let mut reader = BufReader::new(std::fs::File::open(&path).unwrap());
                reader.seek(SeekFrom::Start(at)).unwrap();
                let mut skipped = Vec::new();
                at + reader.read_until(b'\n', &mut skipped).unwrap() as u64
            }
        };
        let started = std::time::Instant::now();
        let mut slowest = std::time::Duration::ZERO;
        for page in 0..pages {
            append(
                &path,
                &(reply(&format!("bench-{page}"), "bench-parent") + "\n"),
            );
            let read_started = std::time::Instant::now();
            let value = crate::session_chat_history::read_history(
                crate::session_chat::SessionChatTranscriptAgent::Claude,
                &path,
                cursor,
                200,
                false,
                false,
                "bench-project",
                "bench-session",
            )
            .unwrap();
            let took = read_started.elapsed();
            slowest = slowest.max(took);
            let next = value["beforeOffset"].as_u64().unwrap();
            let rows = value["messages"].as_array().map_or(0, Vec::len);
            eprintln!(
                "page {page:>3}: {:>8.1} ms, {rows:>4} rows, cursor {cursor} -> {next} ({:.1} MB from the end)",
                took.as_secs_f64() * 1000.0,
                (original_size - next) as f64 / 1_048_576.0,
            );
            if next == cursor || value["hasMore"] != true {
                break;
            }
            cursor = next;
        }
        eprintln!(
            "total {:.1} ms, slowest page {:.1} ms",
            started.elapsed().as_secs_f64() * 1000.0,
            slowest.as_secs_f64() * 1000.0
        );
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(original_size)
            .unwrap();
    }
}
