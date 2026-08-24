// C1 wave-3 extraction: the CommandPaneTabGroup and WorkspaceTabGroup structs and impls moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::*;

pub(crate) struct CommandPaneTabGroup {
    pub(crate) tabs: Vec<CommandPaneTab>,
    pub(crate) active_session: CommandSessionId,
}

#[derive(Clone)]
pub(crate) struct WorkspaceTabGroup {
    pub(crate) tabs: Vec<WorkspaceTab>,
    pub(crate) active_tab: TerminalSessionId,
}

impl CommandPaneTabGroup {
    pub(crate) fn active_session_id(&self) -> Option<CommandSessionId> {
        self.tabs
            .iter()
            .find(|tab| tab.session_id == self.active_session)
            .or_else(|| self.tabs.first())
            .map(|tab| tab.session_id)
    }

    pub(crate) fn active_session_index(&self) -> Option<usize> {
        let active_session_id = self.active_session_id()?;
        self.tabs
            .iter()
            .position(|tab| tab.session_id == active_session_id)
    }

    pub(crate) fn has_session(&self, session_id: CommandSessionId) -> bool {
        self.tabs.iter().any(|tab| tab.session_id == session_id)
    }

    pub(crate) fn cycle_active_session(&mut self, reverse: bool) -> Option<CommandSessionId> {
        if self.tabs.is_empty() {
            return None;
        }

        let current_index = self
            .tabs
            .iter()
            .position(|tab| tab.session_id == self.active_session)
            .unwrap_or(0);
        let next_index = if reverse {
            current_index
                .checked_sub(1)
                .unwrap_or(self.tabs.len().saturating_sub(1))
        } else {
            (current_index + 1) % self.tabs.len()
        };
        self.active_session = self.tabs[next_index].session_id;
        Some(self.active_session)
    }

    pub(crate) fn remove_session(
        &mut self,
        session_id: CommandSessionId,
    ) -> Option<CommandPaneTab> {
        let tab_index = self
            .tabs
            .iter()
            .position(|tab| tab.session_id == session_id)?;
        let tab = self.tabs.remove(tab_index);

        if self.active_session == session_id
            && let Some(next_active_tab) = self.tabs.get(tab_index).or_else(|| self.tabs.last())
        {
            self.active_session = next_active_tab.session_id;
        }

        Some(tab)
    }

    pub(crate) fn insert_session_at(&mut self, tab: CommandPaneTab, insertion_index: usize) {
        let mut target_index = insertion_index.min(self.tabs.len());

        if let Some(existing_index) = self
            .tabs
            .iter()
            .position(|candidate| candidate.session_id == tab.session_id)
        {
            let existing_tab = self.tabs.remove(existing_index);
            if existing_index < target_index {
                target_index -= 1;
            }
            self.tabs
                .insert(target_index.min(self.tabs.len()), existing_tab);
        } else {
            self.tabs.insert(target_index, tab);
        }
    }
}

impl WorkspaceTabGroup {
    pub(crate) fn active_session_id(&self) -> Option<TerminalSessionId> {
        self.tabs
            .iter()
            .find(|tab| tab.session_id == self.active_tab)
            .or_else(|| self.tabs.first())
            .map(|tab| tab.session_id)
    }

    pub(crate) fn active_session_index(&self) -> Option<usize> {
        let active_session_id = self.active_session_id()?;
        self.tabs
            .iter()
            .position(|tab| tab.session_id == active_session_id)
    }

    pub(crate) fn has_session(&self, session_id: TerminalSessionId) -> bool {
        self.tabs.iter().any(|tab| tab.session_id == session_id)
    }

    pub(crate) fn cycle_active_session(&mut self, reverse: bool) -> Option<TerminalSessionId> {
        if self.tabs.is_empty() {
            return None;
        }

        let current_index = self
            .tabs
            .iter()
            .position(|tab| tab.session_id == self.active_tab)
            .unwrap_or(0);
        let next_index = if reverse {
            current_index
                .checked_sub(1)
                .unwrap_or(self.tabs.len().saturating_sub(1))
        } else {
            (current_index + 1) % self.tabs.len()
        };
        self.active_tab = self.tabs[next_index].session_id;
        Some(self.active_tab)
    }

    pub(crate) fn remove_session(&mut self, session_id: TerminalSessionId) -> Option<WorkspaceTab> {
        let tab_index = self
            .tabs
            .iter()
            .position(|tab| tab.session_id == session_id)?;
        let tab = self.tabs.remove(tab_index);

        if self.active_tab == session_id
            && let Some(next_active_tab) = self.tabs.get(tab_index).or_else(|| self.tabs.last())
        {
            self.active_tab = next_active_tab.session_id;
        }

        Some(tab)
    }

    pub(crate) fn insert_session_at(&mut self, tab: WorkspaceTab, insertion_index: usize) {
        let mut target_index = insertion_index.min(self.tabs.len());

        if let Some(existing_index) = self
            .tabs
            .iter()
            .position(|candidate| candidate.session_id == tab.session_id)
        {
            let existing_tab = self.tabs.remove(existing_index);
            if existing_index < target_index {
                target_index -= 1;
            }
            self.tabs
                .insert(target_index.min(self.tabs.len()), existing_tab);
        } else {
            self.tabs.insert(target_index, tab);
        }
    }
}
