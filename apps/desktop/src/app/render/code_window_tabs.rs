//! The Code view's window tabs: a tab per VS Code window (the project's own first, then its extra
//! windows) and a `+` for a new one, the way VS Code shows its windows as tabs on macOS
//! (`window.nativeTabs`), drawn by the shared row (`view_instance_tabs.rs`). See
//! `app/workarea/code_windows.rs`.

use gpui::AnyElement;
use gpui::SharedString;

use crate::app::model::*;
use crate::app::render::view_instance_tabs::{ViewInstance, ViewInstanceRow, ViewInstanceTab};
use crate::*;

impl GhostexGpuiApp {
    /// The row, whenever VS Code windows as tabs is on: with only the project's own window it is
    /// that tab and the `+`. The project's own window has no close control; it closes with the
    /// VS Code tab. Each extra window shows its folder or workspace, with VS Code's window title as its
    /// tooltip.
    ///
    /// CDXC:CodeEditor 2026-10-24 WHY:
    /// With the row hidden until a second window existed, and no VS Code Window row in the side panel's `+` menu, there was no visible way to open one. The row stays while the setting is on, so its `+` is always there; this replaces the same day's macOS-style rule that showed it only with two windows or more.
    pub(crate) fn render_code_window_tab_row(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        if !self.code_windows_as_tabs_active() {
            return None;
        }
        let project_label: SharedString = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .map(|snapshot| snapshot.display_name.clone())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| TitlebarMode::Source.tab_label())
            .into();
        let active = self.code_windows.active;
        let own = ViewInstanceTab {
            instance: ViewInstance::CodeWindow(None),
            tooltip: project_label.clone(),
            label: project_label,
            showing: active.is_none(),
            closable: false,
            unsaved: false,
            icon: None,
            asleep: false,
            pinned: false,
            drag: None,
        };
        let tabs =
            std::iter::once(own).chain(self.code_windows.windows.iter().map(|code_window| {
                let label = code_window.label();
                ViewInstanceTab {
                    instance: ViewInstance::CodeWindow(Some(code_window.id)),
                    tooltip: code_window.title.clone().unwrap_or_else(|| label.clone()),
                    label,
                    showing: active == Some(code_window.id),
                    closable: true,
                    unsaved: false,
                    icon: None,
                    asleep: false,
                    pinned: false,
                    drag: None,
                }
            }));
        Some(self.render_view_instance_tab_row(
            ViewInstanceRow::CodeWindows,
            tabs,
            &self.code_window_tab_scroll_handle,
            cx,
        ))
    }
}
