//! Up-arrow recall of what this machine's composers have sent.
//!
//! Port of `packages/core-ui/chat/session-chat-composer-state.ts`, minus its
//! `pushSessionChatComposerHistory`: the native host never pushed locally, it re-read. The ring
//! is filled by `composer('history')` alone (`native-host.ts:1210`), which the HOST answers from
//! the `sentHistory` store `composer('submitted')` writes, so a prompt sent in another chat is in
//! the ring here too and this crate has no writer for it by design.

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

    /// One entry newer, or back to a blank composer. `None` when no entry is showing.
    pub fn recall_next(&mut self) -> Option<String> {
        let index = self.index? + 1;
        if index >= self.entries.len() {
            // Back to blank.
            self.index = None;
            return Some(String::new());
        }
        self.index = Some(index);
        Some(self.entries.get(index).cloned().unwrap_or_default())
    }

    /// Any manual edit resets the recall cursor and keeps the entries.
    pub fn reset_index(&mut self) {
        self.index = None;
    }

    /// Whether the composer is currently showing a recalled entry.
    pub fn is_active(&self) -> bool {
        self.index.is_some()
    }
}
