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
            !self
                .row_offset
                .get(parent_id)
                .is_some_and(|offset| *offset >= before_offset)
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
    scan_session_chat_branch_boundary(file_path, end_offset, decode).unwrap_or_default()
}

/*
Forward re-derivation of the state at `end_offset` from the rows above it. Only
rows that can carry a prompt are decoded; every other row is read for its
`uuid`/`parentUuid` alone, because all the pass needs from them is whether they
resolve an id the page below is still waiting for.
*/
fn scan_session_chat_branch_boundary(
    file_path: &Path,
    end_offset: u64,
    decode: SessionChatLineDecoder,
) -> std::io::Result<SessionChatBranchBoundary> {
    let mut file = std::fs::File::open(file_path)?;
    file.seek(SeekFrom::Start(end_offset))?;
    let mut reader = BufReader::new(file);
    let mut offset = end_offset;
    let mut ids_seen: HashSet<String> = HashSet::new();
    let mut prompt_parents: HashSet<String> = HashSet::new();
    let mut marker_in_force: Option<TranscriptLeafMarker> = None;
    let mut pending = session_chat_pending_rewind(file_path).map(|pending| BranchCutoff {
        leaf: pending.leaf_id,
        pending_from: Some(pending.cutoff_offset),
    });
    let mut pending_retired = false;
    loop {
        let line_offset = offset;
        let mut bytes = Vec::new();
        let read = reader.read_until(b'\n', &mut bytes)?;
        if read == 0 {
            break;
        }
        offset += read as u64;
        if bytes.last() == Some(&b'\n') {
            bytes.pop();
        }
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        if bytes.is_empty() {
            continue;
        }
        let line = String::from_utf8_lossy(&bytes).into_owned();
        let Some(record) = parse_json_object(&line) else {
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
        ids_seen.insert(row.id.clone());
        if !claude_record_type_can_be_prompt(&record) {
            continue;
        }
        if !transcript_message_is_branch_prompt(decode(&line, &fallback_id).as_ref()) {
            continue;
        }
        if let Some(parent_id) = row.parent_id.clone() {
            prompt_parents.insert(parent_id);
        }
        if let Some(cutoff) = pending.as_ref() {
            if !pending_retired && cutoff.pending_from.is_some_and(|from| line_offset >= from) {
                pending_retired = true;
                if row.parent_id.as_deref() != cutoff.leaf.as_deref() {
                    pending = None;
                }
            }
        }
    }
    // A parent whose own row is above the boundary was already resolved there,
    // and a cutoff whose leaf is above it has nothing left to retract below.
    prompt_parents.retain(|parent_id| !ids_seen.contains(parent_id));
    let cutoff = marker_in_force
        .as_ref()
        .map(BranchCutoff::from_marker)
        .or(pending)
        .filter(|cutoff| {
            cutoff
                .leaf
                .as_ref()
                .is_none_or(|leaf| !ids_seen.contains(leaf))
        });
    Ok(SessionChatBranchBoundary {
        prompt_parents,
        cutoff,
    })
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
