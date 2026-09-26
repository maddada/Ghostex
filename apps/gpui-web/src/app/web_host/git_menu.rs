//! The Git menu in the page. The desktop draws it as a titlebar popup window from the state `gx_store/git/hud.rs` publishes (`titlebar_git_menu_state`); the page draws the same sections and rows as a dropdown under the header's Commit button, and every row runs the desktop's Git action through `gx_store_git_titlebar_action`. It opens with the last state read and asks for a fresh one, as the desktop's does.
use gpui::{
    AnyElement, Context, Hsla, IntoElement, SharedString, Window, deferred, div, prelude::*, px,
};

use crate::GhostexGpuiApp;
use crate::app::consts::GPUI_TITLEBAR_GIT_ACTION_REFRESH_SELECTOR;
use crate::app::helpers::*;

const MENU_WIDTH: f32 = 240.0;
const ROW_HEIGHT: f32 = 30.0;

impl GhostexGpuiApp {
    /// The header's Commit button.
    pub(crate) fn web_toggle_git_menu(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.web_host.git_menu_open = !self.web_host.git_menu_open;
        if self.web_host.git_menu_open {
            self.gx_store_git_titlebar_action(GPUI_TITLEBAR_GIT_ACTION_REFRESH_SELECTOR, cx);
        }
        cx.notify();
    }

    /// The desktop's name for "the Git state changed while the menu is open": the dropdown redraws from the new state.
    pub(crate) fn refresh_open_git_popup(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.web_host.git_menu_open {
            cx.notify();
        }
    }

    fn web_git_menu_row(
        &self,
        id: SharedString,
        label: String,
        selector: Option<&'static str>,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text: Hsla = titlebar_popup_menu_foreground();
        let row = div()
            .id(id)
            .h(px(ROW_HEIGHT))
            .px(px(10.0))
            .mx(px(4.0))
            .flex()
            .items_center()
            .rounded(px(6.0))
            .text_size(px(12.5))
            .text_color(if disabled { text.opacity(0.45) } else { text })
            .child(label);
        match selector.filter(|_| !disabled) {
            Some(selector) => row
                .cursor_pointer()
                .hover(|row| row.bg(titlebar_popup_menu_hover_color()))
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.web_host.git_menu_open = false;
                    app.gx_store_git_titlebar_action(selector, cx);
                    cx.notify();
                }))
                .into_any_element(),
            None => row.into_any_element(),
        }
    }

    fn web_git_menu_heading(label: &'static str) -> AnyElement {
        div()
            .h(px(24.0))
            .px(px(14.0))
            .flex()
            .items_end()
            .pb(px(3.0))
            .text_size(px(11.0))
            .text_color(titlebar_popup_menu_foreground().opacity(0.55))
            .child(label)
            .into_any_element()
    }

    /// The open dropdown, right-aligned under the Commit button; nothing while it is closed.
    pub(crate) fn render_web_git_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.web_host.git_menu_open {
            return None;
        }
        let trigger = self.web_host.git_button_bounds.get()?;
        let mut rows: Vec<AnyElement> = Vec::new();
        match self.titlebar_git_menu_state.as_ref() {
            Some(state) if !state.is_busy && state.is_repo => {
                rows.push(Self::web_git_menu_heading("Status"));
                rows.push(
                    self.web_git_menu_row(
                        "web-git-branch".into(),
                        state
                            .branch
                            .clone()
                            .unwrap_or_else(|| "(detached HEAD)".to_string()),
                        None,
                        true,
                        cx,
                    ),
                );
                rows.push(self.web_git_menu_row(
                    "web-git-changes".into(),
                    format!("+{} -{}", state.additions, state.deletions),
                    None,
                    true,
                    cx,
                ));
                let sync_disabled = state.sync_remote_disabled
                    || (state.ahead_count == 0 && state.behind_count == 0);
                rows.push(self.web_git_menu_row(
                    "web-git-commits".into(),
                    format!("{} ahead, {} behind", state.ahead_count, state.behind_count),
                    Some("syncRemote"),
                    sync_disabled,
                    cx,
                ));
                rows.push(
                    div()
                        .h(px(1.0))
                        .my(px(6.0))
                        .mx(px(8.0))
                        .bg(titlebar_popup_menu_border_color())
                        .into_any_element(),
                );
                rows.push(Self::web_git_menu_heading("Actions"));
                for (index, row) in state.rows.iter().enumerate() {
                    rows.push(self.web_git_menu_row(
                        SharedString::from(format!("web-git-action-{index}")),
                        row.label.clone(),
                        Some(row.action.selector()),
                        row.disabled,
                        cx,
                    ));
                }
            }
            Some(state) if !state.is_busy => rows.push(self.web_git_menu_row(
                "web-git-not-repo".into(),
                "Not a Git repository".to_string(),
                None,
                true,
                cx,
            )),
            _ => rows.push(self.web_git_menu_row(
                "web-git-loading".into(),
                "Loading Git state...".to_string(),
                None,
                true,
                cx,
            )),
        }
        let left = trigger.right() - px(MENU_WIDTH);
        let top = trigger.bottom() + px(5.0);
        Some(
            deferred(
                div()
                    .id("web-git-menu")
                    .absolute()
                    .left(left)
                    .top(top)
                    .w(px(MENU_WIDTH))
                    .py(px(4.0))
                    .flex()
                    .flex_col()
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(titlebar_popup_menu_border_color())
                    .bg(titlebar_popup_menu_background())
                    .shadow_lg()
                    .occlude()
                    .on_mouse_down_out(cx.listener(|app, _, window, cx| {
                        // A press on the Commit button itself is its own toggle.
                        let on_trigger = app
                            .web_host
                            .git_button_bounds
                            .get()
                            .is_some_and(|bounds| bounds.contains(&window.mouse_position()));
                        if !on_trigger {
                            app.web_host.git_menu_open = false;
                            cx.notify();
                        }
                    }))
                    .children(rows),
            )
            .with_priority(30)
            .into_any_element(),
        )
    }
}
