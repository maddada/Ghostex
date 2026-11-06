//! The Add Project dialog's keyboard model: the list highlight, Enter and Backspace in the path
//! input, the Tab order of the chrome buttons, and the preview binary's scripted steps.
use super::window::*;
use gpui::Focusable as _;
use gpui::{App, Context, FocusHandle, Window};

impl GpuiAddProjectModalWindow {
    pub(super) fn set_highlight(
        &mut self,
        value: Option<String>,
        d: &Derived,
        cx: &mut Context<Self>,
    ) {
        if self.highlight == value {
            return;
        }
        if let Some(index) = value
            .as_deref()
            .and_then(|value| d.rows.iter().position(|row| row.value == value))
        {
            // The group label is the list's first child.
            self.list_scroll.scroll_to_item(index + 1);
        }
        self.highlight = value;
        cx.notify();
    }

    /// `onMouseMove` on a row: the pointer's row becomes the highlight.
    pub(super) fn hover_row(&mut self, value: &str, cx: &mut Context<Self>) {
        if self.highlight.as_deref() != Some(value) {
            self.highlight = Some(value.to_string());
            self.highlight_from_keys = false;
            cx.notify();
        }
    }

    pub(super) fn move_highlight(&mut self, direction: isize, cx: &mut Context<Self>) {
        let d = self.derive();
        let selectable: Vec<&AddProjectRow> = d.selectable_rows().collect();
        if selectable.is_empty() {
            return;
        }
        let current = selectable
            .iter()
            .position(|row| Some(row.value.as_str()) == self.highlight.as_deref());
        let next = match current {
            None if direction > 0 => Some(selectable[0].value.clone()),
            None => Some(selectable[selectable.len() - 1].value.clone()),
            Some(index) => {
                let next = index as isize + direction;
                (next >= 0 && (next as usize) < selectable.len())
                    .then(|| selectable[next as usize].value.clone())
            }
        };
        self.set_highlight(next, &d, cx);
        self.highlight_from_keys = true;
    }

    /// `handleKeyDown` for Enter: `primary` is Cmd on macOS and Ctrl elsewhere.
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
        // The form: Enter opens the row the arrow keys highlighted, else makes the project;
        // Cmd/Ctrl+Enter always makes it.
        if self.is_new_project_view() {
            let highlighted = d
                .selectable_rows()
                .find(|row| Some(row.value.as_str()) == self.highlight.as_deref())
                .map(|row| row.action.clone());
            match highlighted.filter(|_| !primary && self.highlight_from_keys) {
                Some(action) => self.select_row(action, window, cx),
                None => self.submit_new_project(window, cx),
            }
            return;
        }
        if d.is_repository_step {
            self.submit_repository(window, cx);
            return;
        }
        if d.can_submit_browse_path && (!d.has_highlighted_browse_item || primary) {
            self.submit_resolved_path(window, cx);
            return;
        }
        if let Some(row) = d
            .selectable_rows()
            .find(|row| Some(row.value.as_str()) == self.highlight.as_deref())
        {
            let action = row.action.clone();
            self.select_row(action, window, cx);
        }
    }

    /// Backspace on an empty input steps back; returns false when the input should delete.
    pub(super) fn press_backspace(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(name) = &self.new_folder_name {
            if name.is_empty() {
                self.cancel_new_folder(window, cx);
                return true;
            }
            return false;
        }
        if let Some(form) = self
            .new_project
            .as_ref()
            .filter(|_| self.is_new_project_view())
        {
            if !form.name.is_empty() {
                return false;
            }
            self.pop_view();
            self.settle(window, cx);
            return true;
        }
        if self.query.is_empty() && self.view_stack.len() > 1 {
            self.pop_view();
            self.settle(window, cx);
            return true;
        }
        false
    }

    pub(super) fn chrome_focus_handle(
        &mut self,
        slot: FocusSlot,
        cx: &mut Context<Self>,
    ) -> FocusHandle {
        self.chrome_focus
            .entry(slot)
            .or_insert_with(|| cx.focus_handle())
            .clone()
    }

    /// The Tab order of the current step, the same elements the React dialog's focus trap walks.
    fn focus_order(&self, d: &Derived) -> Vec<FocusSlot> {
        if self.clone_step() == Some(CloneStep::Review) {
            let idle = self.busy.is_none();
            let mut order = Vec::new();
            if idle {
                order.extend([
                    FocusSlot::ReviewBack,
                    FocusSlot::BranchInput,
                    FocusSlot::CloneMainOnly,
                    FocusSlot::ShallowClone,
                    FocusSlot::FooterBack,
                ]);
            }
            if self.can_clone() {
                order.push(FocusSlot::FooterClone);
            }
            return order;
        }
        let mut order = Vec::new();
        if d.is_new_folder_step || d.can_pop_view {
            order.push(FocusSlot::PathBack);
        }
        order.push(FocusSlot::PathInput);
        for (index, row) in d.rows.iter().enumerate() {
            if row.setup_required.is_some() {
                order.push(FocusSlot::SetupRequired(index));
            }
        }
        order
    }

    fn slot_is_focused(&self, slot: FocusSlot, window: &Window, cx: &App) -> bool {
        match slot {
            FocusSlot::PathInput => self.path_input.read(cx).focus_handle(cx).is_focused(window),
            FocusSlot::BranchInput => self
                .branch_input
                .read(cx)
                .focus_handle(cx)
                .is_focused(window),
            _ => self
                .chrome_focus
                .get(&slot)
                .is_some_and(|handle| handle.is_focused(window)),
        }
    }

    pub(super) fn cycle_focus(
        &mut self,
        direction: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let d = self.derive();
        let order = self.focus_order(&d);
        if order.is_empty() {
            return;
        }
        let current = order
            .iter()
            .position(|slot| self.slot_is_focused(*slot, window, cx));
        let next = match current {
            Some(index) => (index as isize + direction).rem_euclid(order.len() as isize) as usize,
            None if direction > 0 => 0,
            None => order.len() - 1,
        };
        match order[next] {
            FocusSlot::PathInput => self.focus_path_input(window, cx),
            FocusSlot::BranchInput => self
                .branch_input
                .update(cx, |input, cx| input.focus(window, cx)),
            slot => {
                let handle = self.chrome_focus_handle(slot, cx);
                handle.focus(window, cx);
            }
        }
        cx.notify();
    }
}
