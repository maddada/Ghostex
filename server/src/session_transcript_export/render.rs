use super::*;

// ---------------------------------------------------------------------------
// Markdown rendering
// ---------------------------------------------------------------------------

const UNTITLED_EXPORT_TITLE: &str = "Untitled session";
const TITLE_MAX_CHARS: usize = 80;

pub(super) fn export_title(session_title: Option<&str>, transcript: &ParsedTranscript) -> String {
    if let Some(title) = session_title
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return clamp_title(title);
    }
    if let Some(title) = transcript.meta.title.as_deref() {
        return clamp_title(title);
    }
    let first_prompt = transcript
        .entries
        .iter()
        .find(|entry| entry.section == TranscriptExportSection::UserMessage)
        .map(|entry| entry.text.as_str())
        .unwrap_or_default();
    let first_line = first_prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    if first_line.is_empty() {
        return UNTITLED_EXPORT_TITLE.to_string();
    }
    clamp_title(first_line)
}

fn clamp_title(title: &str) -> String {
    let normalized = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= TITLE_MAX_CHARS {
        return normalized;
    }
    let clipped: String = normalized.chars().take(TITLE_MAX_CHARS).collect();
    format!("{clipped}…")
}

/*
Message bodies are emitted verbatim, headings and all, so agent prose that
contains its own `##` sits at the same outline level as the structural
`## 👤 User` / `## 🤖 Agent` headings. That is deliberate: demoting either side
means rewriting message text, and a leading `#` inside a fenced block is a shell
comment, not a heading, so the rewrite would corrupt content to tidy an outline
no one navigates — this file is read by the next agent, not skimmed in a TOC.
The reference exporter (`codex-md.py`) makes the same call.
*/
pub(super) fn render_markdown(
    title: &str,
    transcript: &ParsedTranscript,
    selection: &SessionTranscriptExportSelection,
) -> String {
    render_parts(title, transcript, selection).assemble(selection.last_turns)
}

/// The file in pieces: the header, what comes before the first message from the user, and one
/// piece per turn, so a handoff of the last turns and the sizes the dialog estimates are cut from
/// one rendering.
///
/// CDXC:TranscriptExport 2026-11-03 DECISION:
/// User (Q1151): a step of the History slider is a turn: your message and everything up to your next one, so a reply never goes without its question. (Q1152): tool calls are no steps; Include decides whether they print inside the kept turns. (Q1157): the turns before the kept ones are not dropped silently: your messages from them stay, one line each, oldest first, without the agent's replies.
pub(super) struct RenderedParts {
    head: String,
    preamble: String,
    turns: Vec<String>,
    /// Each turn's message from the user, on one line, for the list of earlier messages.
    prompts: Vec<String>,
}

impl RenderedParts {
    /// The header's length: [`assemble`](Self::assemble) starts with it.
    pub(super) fn head_len(&self) -> usize {
        self.head.len()
    }

    pub(super) fn turn_count(&self) -> usize {
        self.turns.len()
    }

    /// How many turns go when the selection asks for `last_turns`.
    fn kept(&self, last_turns: Option<usize>) -> usize {
        last_turns.map_or(self.turns.len(), |turns| turns.min(self.turns.len()))
    }

    fn earlier_block(&self, kept: usize) -> String {
        let left_out = self.turns.len() - kept;
        if left_out == 0 {
            return String::new();
        }
        let listed = &self.prompts[left_out.saturating_sub(EARLIER_PROMPTS_MAX)..left_out];
        let mut block = String::from("## ⏪ Earlier in this conversation\n\n");
        let turns = if left_out == 1 {
            "turn is"
        } else {
            "turns are"
        };
        if listed.len() < left_out {
            block.push_str(&format!(
                "The {left_out} earlier {turns} left out. Your messages from the last {} of them, oldest first:\n\n",
                listed.len()
            ));
        } else {
            block.push_str(&format!(
                "The {left_out} earlier {turns} left out. Your messages from them, oldest first:\n\n"
            ));
        }
        for prompt in listed {
            block.push_str(&format!("- {prompt}\n"));
        }
        block.push('\n');
        block
    }

    pub(super) fn assemble(&self, last_turns: Option<usize>) -> String {
        let kept = self.kept(last_turns);
        let mut out = self.head.clone();
        if kept == self.turns.len() {
            out.push_str(&self.preamble);
        } else {
            out.push_str(&self.earlier_block(kept));
        }
        for turn in &self.turns[self.turns.len() - kept..] {
            out.push_str(turn);
        }
        out
    }

    /// The file's size for each "last N turns", N from 1 up, as [`assemble`](Self::assemble)
    /// would write it.
    pub(super) fn last_turns_bytes(&self) -> Vec<usize> {
        let mut tail = 0;
        (1..=self.turns.len())
            .map(|kept| {
                tail += self.turns[self.turns.len() - kept].len();
                let before = if kept == self.turns.len() {
                    self.preamble.len()
                } else {
                    self.earlier_block(kept).len()
                };
                self.head.len() + before + tail
            })
            .collect()
    }

    pub(super) fn whole_bytes(&self) -> usize {
        self.head.len() + self.preamble.len() + self.turns.iter().map(String::len).sum::<usize>()
    }
}

pub(super) fn render_parts(
    title: &str,
    transcript: &ParsedTranscript,
    selection: &SessionTranscriptExportSelection,
) -> RenderedParts {
    let mut head = format!("# {title}\n\n");
    if selection.includes(TranscriptExportSection::SessionMeta) {
        head.push_str(&render_session_meta(title, &transcript.meta));
    }
    let cwd = transcript.meta.cwd.as_deref();
    let starts: Vec<usize> = transcript
        .entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.section == TranscriptExportSection::UserMessage)
        .map(|(index, _)| index)
        .collect();
    let first = starts.first().copied().unwrap_or(transcript.entries.len());
    let preamble = render_entries(&transcript.entries[..first], selection, cwd);
    let mut turns: Vec<String> = starts
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let end = starts
                .get(index + 1)
                .copied()
                .unwrap_or(transcript.entries.len());
            render_entries(&transcript.entries[*start..end], selection, cwd)
        })
        .collect();
    let prompts = starts
        .iter()
        .map(|start| earlier_prompt_line(&transcript.entries[*start].text))
        .collect();
    // The file ends on one blank line, whichever piece is last.
    let mut preamble = preamble;
    let trailing = match turns.last_mut() {
        Some(turn) => turn,
        None => &mut preamble,
    };
    while trailing.ends_with("\n\n\n") {
        trailing.pop();
    }
    RenderedParts {
        head,
        preamble,
        turns,
        prompts,
    }
}

fn earlier_prompt_line(text: &str) -> String {
    let line = one_line(text);
    if line.is_empty() {
        return "(no text)".to_string();
    }
    if line.chars().count() <= EARLIER_PROMPT_MAX_CHARS {
        return line;
    }
    let clipped: String = line.chars().take(EARLIER_PROMPT_MAX_CHARS).collect();
    format!("{clipped}…")
}

fn render_entries(
    entries: &[ExportEntry],
    selection: &SessionTranscriptExportSelection,
    cwd: Option<&str>,
) -> String {
    let mut out = String::new();
    /*
    Patch lines are buffered instead of written per entry: a turn that edits one
    file seven times in a row produced seven near-identical `🔧 path` lines, and
    the next agent has to read past all of them to reach the next real step.
    Consecutive changes to the same file merge into one line with summed counts;
    the first entry that renders anything else — a failure note included — flushes
    the run, so a failure always stays attached to the patch that failed.
    */
    let mut patch_run: Vec<PatchFileChange> = Vec::new();
    for entry in entries {
        if !selection.includes(entry.section) {
            continue;
        }
        if !matches!(
            entry.section,
            TranscriptExportSection::Patch | TranscriptExportSection::PatchOutput
        ) {
            flush_patch_run(&mut out, &mut patch_run, cwd);
        }
        match entry.section {
            TranscriptExportSection::SessionMeta => {}
            TranscriptExportSection::UserMessage => {
                out.push_str(&format!("## 👤 User\n\n{}\n\n", entry.text.trim_end()));
            }
            TranscriptExportSection::AgentMessage => {
                out.push_str(&format!("## 🤖 Agent\n\n{}\n\n", entry.text.trim_end()));
            }
            TranscriptExportSection::AgentReasoning => {
                out.push_str(&format!(
                    "> 🧠 **Reasoning:** {}\n\n",
                    one_line(&entry.text)
                ));
            }
            TranscriptExportSection::InternalReasoning => {
                out.push_str(&format!(
                    "> 🔒 **Internal reasoning:** {}\n\n",
                    one_line(&entry.text)
                ));
            }
            TranscriptExportSection::TerminalCmd => {
                out.push_str(&render_tool_call("💻", entry, "bash"));
            }
            TranscriptExportSection::McpCall => {
                out.push_str(&render_tool_call("🔌", entry, "json"));
            }
            TranscriptExportSection::OtherTool => {
                out.push_str(&render_tool_call("🧩", entry, "json"));
            }
            TranscriptExportSection::TerminalOutput
            | TranscriptExportSection::McpOutput
            | TranscriptExportSection::OtherToolOutput => {
                out.push_str(&render_output(entry, selection.terminal_output_tail_lines));
            }
            TranscriptExportSection::Patch => {
                if entry.patch.is_empty() || entry.patch_failure.is_some() {
                    // A failed (or file-less) patch prints on its own, so the
                    // run in front of it has to land first or the summaries
                    // would appear after the patch that follows them.
                    flush_patch_run(&mut out, &mut patch_run, cwd);
                    out.push_str(&render_patch(entry, cwd));
                } else {
                    for change in &entry.patch {
                        merge_patch_change(&mut patch_run, change);
                    }
                }
            }
            TranscriptExportSection::PatchOutput => {
                // Patch results are folded into the patch summary (plan Q4).
            }
            TranscriptExportSection::WebSearch => out.push_str("> 🔍 **Web search**\n\n"),
            TranscriptExportSection::TokenCount => {
                out.push_str(&format!("> 📊 **Tokens:** {}\n\n", one_line(&entry.text)));
            }
            TranscriptExportSection::TurnContext => {
                out.push_str(&format!("> 🔄 **Turn:** {}\n\n", one_line(&entry.text)));
            }
            TranscriptExportSection::TaskEvent => {
                out.push_str(&format!("> 📌 **{}**\n\n", one_line(&entry.text)));
            }
            TranscriptExportSection::SystemMessage => {
                out.push_str(&format!(
                    "### ⚙️ System message\n\n{}\n\n",
                    fenced_block(&entry.text, "text")
                ));
            }
            TranscriptExportSection::GitSnapshot => {
                out.push_str(&format!("> 📸 **Snapshot:** {}\n\n", one_line(&entry.text)));
            }
            TranscriptExportSection::SessionEvent => {
                out.push_str(&format!("> 🔔 **{}**\n\n", one_line(&entry.text)));
            }
        }
    }
    flush_patch_run(&mut out, &mut patch_run, cwd);
    out
}

fn flush_patch_run(out: &mut String, run: &mut Vec<PatchFileChange>, cwd: Option<&str>) {
    for change in run.drain(..) {
        out.push_str(&format!("🔧 {}\n\n", patch_change_line(&change, cwd)));
    }
}

/// Folds a change into the open run when it touches the same file as the line
/// currently being built. Only the LAST line is a merge candidate: alternating
/// edits to two files stay in the order the agent made them.
fn merge_patch_change(run: &mut Vec<PatchFileChange>, change: &PatchFileChange) {
    let Some(open) = run.last_mut().filter(|open| open.path == change.path) else {
        run.push(change.clone());
        return;
    };
    open.added += change.added;
    open.removed += change.removed;
    open.start_line = min_line(open.start_line, change.start_line);
    open.end_line = max_line(open.end_line, change.end_line);
    open.kind = merged_patch_kind(open.kind, change.kind);
}

/// A file created and then edited is still a new file; a file whose last change
/// deletes it is deleted, whatever happened to it before.
fn merged_patch_kind(open: PatchChangeKind, next: PatchChangeKind) -> PatchChangeKind {
    match (open, next) {
        (_, PatchChangeKind::Deleted) => PatchChangeKind::Deleted,
        (PatchChangeKind::Added, _) | (_, PatchChangeKind::Added) => PatchChangeKind::Added,
        _ => PatchChangeKind::Updated,
    }
}

fn min_line(open: Option<usize>, next: Option<usize>) -> Option<usize> {
    match (open, next) {
        (Some(open), Some(next)) => Some(open.min(next)),
        (value, None) | (None, value) => value,
    }
}

fn max_line(open: Option<usize>, next: Option<usize>) -> Option<usize> {
    match (open, next) {
        (Some(open), Some(next)) => Some(open.max(next)),
        (value, None) | (None, value) => value,
    }
}

fn render_session_meta(title: &str, meta: &TranscriptMeta) -> String {
    let mut lines: Vec<String> = vec![
        format!("Title: {title}"),
        format!(
            "Agent: {}",
            meta.agent_name
                .unwrap_or_else(|| agent_display_name(meta.agent))
        ),
    ];
    if let Some(model) = &meta.model {
        lines.push(format!("Model: {model}"));
    }
    if let Some(session_id) = &meta.agent_session_id {
        lines.push(format!("Session: {session_id}"));
    }
    if let Some(cwd) = &meta.cwd {
        lines.push(format!("CWD: {cwd}"));
    }
    if let Some(started_at) = &meta.started_at {
        lines.push(format!("Date: {started_at}"));
    }
    lines.push(format!("Source: {}", meta.source_path.display()));
    format!("```yaml\n{}\n```\n\n", lines.join("\n"))
}

fn render_tool_call(emoji: &str, entry: &ExportEntry, language: &str) -> String {
    let name = entry.tool_name.as_deref().unwrap_or("tool");
    if entry.text.trim().is_empty() {
        return format!("### {emoji} `{name}`\n\n");
    }
    format!(
        "### {emoji} `{name}`\n\n{}\n\n",
        fenced_block(entry.text.trim_end(), language)
    )
}

fn render_output(entry: &ExportEntry, tail_lines: usize) -> String {
    let text = tail_capped(entry.text.trim_end(), tail_lines);
    if text.trim().is_empty() {
        return String::new();
    }
    let label = if entry.is_error { "Error" } else { "Output" };
    format!("**{label}:**\n\n{}\n\n", fenced_block(&text, "text"))
}

/// Keeps the LAST `tail_lines` lines of a block, announcing what was dropped.
fn tail_capped(text: &str, tail_lines: usize) -> String {
    if tail_lines == 0 {
        return text.to_string();
    }
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.len() <= tail_lines {
        return text.to_string();
    }
    let trimmed = lines.len() - tail_lines;
    let kept = lines[lines.len() - tail_lines..].join("\n");
    format!("... ({trimmed} lines trimmed) ...\n{kept}")
}

fn render_patch(entry: &ExportEntry, cwd: Option<&str>) -> String {
    let mut out = String::new();
    if entry.patch.is_empty() {
        let name = entry.tool_name.as_deref().unwrap_or("patch");
        out.push_str(&format!("🔧 `{name}` (no file changes recorded)\n\n"));
    }
    for change in &entry.patch {
        out.push_str(&format!("🔧 {}\n\n", patch_change_line(change, cwd)));
    }
    if let Some(failure) = &entry.patch_failure {
        out.push_str(&format!("⚠ patch failed: {failure}\n\n"));
    }
    out
}

/// Absolute paths inside the session's own working directory are shown
/// relative to it — the next agent runs there, and the full prefix repeated on
/// every patch line buries the part that matters.
fn project_relative_path(path: &str, cwd: Option<&str>) -> String {
    let Some(cwd) = cwd
        .map(|cwd| cwd.trim_end_matches('/'))
        .filter(|cwd| !cwd.is_empty())
    else {
        return path.to_string();
    };
    match path.strip_prefix(&format!("{cwd}/")) {
        Some(relative) if !relative.is_empty() => relative.to_string(),
        _ => path.to_string(),
    }
}

fn patch_change_line(change: &PatchFileChange, cwd: Option<&str>) -> String {
    let mut line = project_relative_path(&change.path, cwd);
    if let (Some(start), Some(end)) = (change.start_line, change.end_line) {
        if start == end {
            line.push_str(&format!(":{start}"));
        } else {
            line.push_str(&format!(":{start}-{end}"));
        }
    }
    match change.kind {
        PatchChangeKind::Added => line.push_str(" (new file)"),
        PatchChangeKind::Deleted => line.push_str(" (deleted)"),
        PatchChangeKind::Updated => {
            line.push_str(&format!(" (+{}/-{})", change.added, change.removed));
        }
    }
    line
}

pub(super) fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Picks a fence longer than any backtick run inside the body, so transcript
/// content that itself contains fenced code cannot break out of the block.
fn fenced_block(text: &str, language: &str) -> String {
    let mut longest_run = 0usize;
    let mut current_run = 0usize;
    for character in text.chars() {
        if character == '`' {
            current_run += 1;
            longest_run = longest_run.max(current_run);
        } else {
            current_run = 0;
        }
    }
    let fence = "`".repeat(longest_run.max(2) + 1);
    format!("{fence}{language}\n{text}\n{fence}")
}

#[cfg(test)]
mod last_turns_tests {
    use super::*;

    fn transcript(turns: usize) -> ParsedTranscript {
        let mut entries = vec![ExportEntry::new(
            TranscriptExportSection::AgentMessage,
            "Hello, ready.",
        )];
        for turn in 1..=turns {
            entries.push(ExportEntry::new(
                TranscriptExportSection::UserMessage,
                format!("Question {turn}\nwith a second line"),
            ));
            entries.push(
                ExportEntry::new(TranscriptExportSection::TerminalCmd, format!("ls {turn}"))
                    .with_tool("bash", None),
            );
            entries.push(ExportEntry::new(
                TranscriptExportSection::AgentMessage,
                format!("Answer {turn}"),
            ));
        }
        ParsedTranscript {
            entries,
            meta: TranscriptMeta {
                agent: SessionChatTranscriptAgent::Codex,
                agent_name: None,
                source_path: std::path::PathBuf::from("/tmp/t.jsonl"),
                agent_session_id: None,
                title: None,
                model: None,
                cwd: None,
                started_at: None,
            },
        }
    }

    fn selection(last: Option<usize>, commands: bool) -> SessionTranscriptExportSelection {
        SessionTranscriptExportSelection::from_include_toggles(commands, false, false)
            .with_last_turns(last)
    }

    #[test]
    fn the_last_turns_keep_each_question_with_its_answer_and_list_the_earlier_messages() {
        let parsed = transcript(5);
        let markdown = render_markdown("T", &parsed, &selection(Some(2), false));
        assert!(markdown.contains("## 👤 User\n\nQuestion 4"), "{markdown}");
        assert!(markdown.contains("Answer 4") && markdown.contains("Answer 5"));
        assert!(!markdown.contains("Answer 3"), "{markdown}");
        assert!(
            !markdown.contains("Hello, ready."),
            "the part before the first message goes too"
        );
        assert!(markdown.contains(
            "## ⏪ Earlier in this conversation\n\nThe 3 earlier turns are left out. Your messages from them, oldest first:\n\n- Question 1 with a second line\n- Question 2 with a second line\n- Question 3 with a second line\n\n"
        ), "{markdown}");
        assert!(markdown.find("⏪").unwrap() < markdown.find("Question 4").unwrap());
    }

    #[test]
    fn tool_calls_are_no_steps_and_follow_include() {
        let parsed = transcript(3);
        let without = render_markdown("T", &parsed, &selection(Some(1), false));
        assert!(!without.contains("ls 3"));
        let with = render_markdown("T", &parsed, &selection(Some(1), true));
        assert!(with.contains("ls 3") && !with.contains("ls 2"), "{with}");
    }

    #[test]
    fn all_of_it_or_more_turns_than_there_are_is_todays_whole_file() {
        let parsed = transcript(4);
        let whole = render_markdown("T", &parsed, &selection(None, true));
        assert!(whole.contains("Hello, ready.") && whole.contains("Question 1"));
        assert!(!whole.contains("⏪"));
        assert_eq!(
            render_markdown("T", &parsed, &selection(Some(4), true)),
            whole
        );
        assert_eq!(
            render_markdown("T", &parsed, &selection(Some(40), true)),
            whole
        );
        assert_eq!(
            render_markdown("T", &parsed, &selection(Some(0), true)),
            whole
        );
        assert!(whole.ends_with("Answer 4\n\n") && !whole.ends_with("\n\n\n"));
    }

    #[test]
    fn the_sizes_the_dialog_estimates_are_the_files_sizes() {
        let parsed = transcript(6);
        let selection = selection(None, true);
        let parts = render_parts("T", &parsed, &selection);
        assert_eq!(parts.turn_count(), 6);
        let sizes = parts.last_turns_bytes();
        for (index, size) in sizes.iter().enumerate() {
            assert_eq!(
                *size,
                parts.assemble(Some(index + 1)).len(),
                "last {}",
                index + 1
            );
        }
        assert_eq!(parts.whole_bytes(), parts.assemble(None).len());
    }

    #[test]
    fn a_long_history_lists_only_its_last_messages_and_says_so() {
        let parsed = transcript(EARLIER_PROMPTS_MAX + 7);
        let markdown = render_markdown("T", &parsed, &selection(Some(1), false));
        assert!(markdown.contains(&format!(
            "The {} earlier turns are left out. Your messages from the last {} of them, oldest first:",
            EARLIER_PROMPTS_MAX + 6,
            EARLIER_PROMPTS_MAX
        )));
        assert!(
            !markdown.contains("- Question 6 with"),
            "the oldest are not listed"
        );
        assert!(markdown.contains("- Question 7 with"));
    }

    #[test]
    fn a_long_message_is_cut_to_one_short_line() {
        let line = earlier_prompt_line(&"word ".repeat(100));
        assert_eq!(line.chars().count(), EARLIER_PROMPT_MAX_CHARS + 1);
        assert!(line.ends_with('…'));
        assert_eq!(earlier_prompt_line("  "), "(no text)");
    }
}
