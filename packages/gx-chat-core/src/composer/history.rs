//! Up-arrow recall of what was sent in this chat, like a shell's history.
//!
//! The ring is filled by `composer('history')` (`native-host.ts:1210`), which the HOST answers from
//! the session's own sent log that `composer('submitted')` appends to, merged here with the user
//! messages the transcript holds ([`merge_transcript`]), so a session sent to before the log
//! existed, or from another client or the terminal, still has a history.
//!
//! CDXC:SessionChat 2026-11-13 DECISION:
//! User: "pressing the up arrow in the chat box doesn't do anything [...] It should load previously sent messages in this session (should keep ones even that were rewound or that are from before I restart the app or whatever)." Up walks back through this session's sent messages, newest first, and Down walks forward and then back to the draft the user had. A recalled message only fills the composer; editing it and sending sends a new message. Supersedes CDXC:SavedPrompts 2026-09-08's "across all sessions" for Up-arrow recall (the Saved prompts Sent tab keeps it).

/// The recall ring: what was sent, and where the cursor sits in it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComposerHistory {
    pub entries: Vec<String>,
    /// `None` means the composer is showing its own text, not a recalled entry.
    pub index: Option<usize>,
    /// The `recallHistory` arm is suspended on `await composer('history')`.
    ///
    /// The ring is read LAZILY, on the first Up arrow with no entry showing
    /// (`native-host.ts:1209`), and never again while the cursor is inside it. The core has no
    /// `await`, so the arm returns here and finishes in family d's settle when the read lands.
    pub loading: bool,
    /// What the composer held when the walk started, given back when Down walks past the newest
    /// entry.
    pub draft: String,
}

impl ComposerHistory {
    /// Adopts what `composer('history')` answered, which replaces the ring wholesale.
    ///
    /// `composerHistory = { entries: await composer('history'), index: null }`.
    pub fn adopt(&mut self, entries: Vec<String>) {
        self.entries = entries;
        self.index = None;
        self.loading = false;
    }

    /// One entry older, or `None` when there is no history at all.
    pub fn recall_previous(&mut self) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }
        let index = match self.index {
            None => self.entries.len() - 1,
            Some(index) => index.saturating_sub(1),
        };
        self.index = Some(index);
        Some(self.entries.get(index).cloned().unwrap_or_default())
    }

    /// One entry newer, or back to the draft the walk started from. `None` when no entry is
    /// showing.
    pub fn recall_next(&mut self) -> Option<String> {
        let index = self.index? + 1;
        if index >= self.entries.len() {
            self.index = None;
            return Some(std::mem::take(&mut self.draft));
        }
        self.index = Some(index);
        Some(self.entries.get(index).cloned().unwrap_or_default())
    }

    /// Any manual edit resets the recall cursor and keeps the entries.
    pub fn reset_index(&mut self) {
        self.index = None;
        self.draft.clear();
    }

    /// Whether the composer is currently showing a recalled entry.
    pub fn is_active(&self) -> bool {
        self.index.is_some()
    }
}

/// The ring the host's sent log answered, with the transcript's user messages it lacks put in
/// front of it, oldest first.
///
/// The log holds only what this computer's chat sent since it existed; a message from before then,
/// from another client or typed in the terminal is still in the transcript. The log wins on order
/// because it also keeps what a rewind took out of the transcript. A message sent twice in a row
/// is one entry, like a shell's history.
pub fn merge_transcript(logged: Vec<String>, transcript: Vec<String>) -> Vec<String> {
    let known: std::collections::BTreeSet<String> =
        logged.iter().map(|text| same_message_key(text)).collect();
    let mut merged: Vec<String> = transcript
        .into_iter()
        .filter(|text| !known.contains(&same_message_key(text)))
        .collect();
    merged.extend(logged);
    merged.retain(|text| !text.trim().is_empty());
    merged.dedup_by(|next, previous| same_message_key(next) == same_message_key(previous));
    merged
}

/// What two copies of one message share: the transcript keeps a message's pictures as blocks
/// where the composer kept them inline, and either may space it differently.
fn same_message_key(text: &str) -> String {
    let mut rest = text;
    let mut kept = String::new();
    while let Some(start) = rest.find("[Image #") {
        kept.push_str(&rest[..start]);
        let after = &rest[start..];
        match after
            .find("](")
            .and_then(|open| after[open..].find(')').map(|close| open + close))
        {
            Some(end) => rest = &after[end + 1..],
            None => {
                kept.push_str(after);
                rest = "";
            }
        }
    }
    kept.push_str(rest);
    let key = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    // A message of pictures alone is told apart by them.
    if key.is_empty() {
        text.trim().to_string()
    } else {
        key
    }
}

/// The user's own messages in the transcript, oldest first, as the composer would hold them: the
/// text, then each picture as the `[Image #N](path)` reference the composer shows as a pill.
/// Rows the chat hides or folds (hook output, slash command echoes) and queued echoes are left out.
pub fn transcript_user_messages<'a>(
    messages: impl Iterator<Item = &'a ghostex_gx_protocol::ChatMessage>,
) -> Vec<String> {
    use ghostex_gx_protocol::{ChatBlock, ChatRole};
    messages
        .filter(|message| {
            message.role == ChatRole::User
                && !message.queued
                && message.startup_delivery.is_none()
                && crate::transcript::noise::classify_suppressed_turn(message).is_none()
        })
        .filter_map(|message| {
            let mut text: Vec<String> = Vec::new();
            let mut images: Vec<String> = Vec::new();
            for block in &message.blocks {
                match block {
                    ChatBlock::Text { text: part } => text.push(part.clone()),
                    ChatBlock::ImageRef { path, url, .. } => {
                        if let Some(target) =
                            path.as_deref().or(url.as_deref()).filter(|t| !t.is_empty())
                        {
                            images.push(format!("[Image #{}]({target})", images.len() + 1));
                        }
                    }
                    _ => {}
                }
            }
            let mut content = text.join("\n").trim().to_string();
            if !images.is_empty() {
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str(&images.join(" "));
            }
            (!content.is_empty()).then_some(content)
        })
        .collect()
}
