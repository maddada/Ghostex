//! The picker's keyboard: the arrow keys move through the open folder's column, Enter opens the
//! highlighted folder (or the typed one, or adds the open folder), Cmd/Ctrl+Enter always adds,
//! Tab completes the typed folder name, and Escape closes what is open, then the dialog.
use super::paths::*;
use super::window::*;
use gpui::{Context, Focusable as _, Window};

impl GpuiAddProjectModalWindow {
    /// Up and Down: the highlight moves through the last column's folders.
    pub(super) fn move_highlight(&mut self, direction: isize, cx: &mut Context<Self>) {
        if self.suggested_mode || self.new_folder_name.is_some() {
            return;
        }
        let d = self.derive();
        let entries: Vec<String> = d
            .filtered
            .filtered
            .iter()
            .filter(|entry| !entry.name.starts_with('.'))
            .map(|entry| entry.full_path.clone())
            .collect();
        if entries.is_empty() {
            return;
        }
        let current = self
            .highlight
            .as_ref()
            .and_then(|path| entries.iter().position(|entry| entry == path));
        let next = match current {
            None if direction > 0 => 0,
            None => entries.len() - 1,
            Some(index) => {
                (index as isize + direction).clamp(0, entries.len() as isize - 1) as usize
            }
        };
        self.highlight = Some(entries[next].clone());
        cx.notify();
    }

    /// Enter in the path field: a repository opens Clone a repo, a highlighted or typed folder
    /// opens, and an open folder (or a typed one that does not exist yet) is added.
    /// `primary` (Cmd on macOS, Ctrl elsewhere) always adds.
    pub(super) fn press_enter(
        &mut self,
        primary: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.new_folder_name.is_some() {
            self.submit_new_folder(window, cx);
            return;
        }
        let d = self.derive();
        if primary {
            self.submit_resolved_path(window, cx);
            return;
        }
        if let Some(clone) = d.clone_detected.as_ref() {
            self.open_clone_sheet(Some(clone.query.clone()), window, cx);
            return;
        }
        if let Some(path) = d
            .filtered
            .highlighted
            .as_ref()
            .map(|entry| entry.full_path.clone())
        {
            self.open_column_entry(&path);
            self.settle(window, cx);
            return;
        }
        if !has_trailing_path_separator(&self.query)
            && let Some(path) = d
                .filtered
                .exact
                .as_ref()
                .map(|entry| entry.full_path.clone())
        {
            self.open_column_entry(&path);
            self.settle(window, cx);
            return;
        }
        self.submit_resolved_path(window, cx);
    }

    /// Escape closes the innermost thing that is open: the new folder's name, the computer menu,
    /// a form's list, the side panel, Suggested; then the dialog.
    pub(super) fn press_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.new_folder_name.is_some() {
            self.cancel_new_folder(window, cx);
        } else if self.machine_menu_open {
            self.machine_menu_open = false;
            cx.notify();
        } else if self.sheet.is_some() {
            self.close_sheet(window, cx);
        } else if self.suggested_mode {
            self.suggested_mode = false;
            self.settle(window, cx);
        } else {
            self.close(window, cx);
        }
    }

    /// Tab in the path field completes the typed folder name to the first match.
    pub(super) fn complete_path(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let d = self.derive();
        if has_trailing_path_separator(&self.query) || !d.is_browsing {
            return;
        }
        let Some(entry) = d
            .filtered
            .highlighted
            .clone()
            .or_else(|| d.filtered.filtered.first().cloned())
        else {
            return;
        };
        let directory = get_browse_directory_path(&self.query);
        self.query = format!(
            "{directory}{}{}",
            entry.name,
            if directory.contains('\\') { '\\' } else { '/' }
        );
        self.highlight = None;
        self.settle(window, cx);
    }

    /// Which of the dialog's text fields has the keyboard.
    pub(super) fn focused_field(&self, window: &Window, cx: &Context<Self>) -> Option<Field> {
        [
            (Field::Path, &self.path_input),
            (Field::FolderName, &self.folder_name_input),
            (Field::Repository, &self.repo_input),
            (Field::Branch, &self.branch_input),
            (Field::ProjectName, &self.project_name_input),
        ]
        .into_iter()
        .find(|(_, input)| input.read(cx).focus_handle(cx).is_focused(window))
        .map(|(field, _)| field)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Field {
    Path,
    FolderName,
    Repository,
    Branch,
    ProjectName,
}
