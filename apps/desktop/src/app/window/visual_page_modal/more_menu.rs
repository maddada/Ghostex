//! The page window's ⋯ menu: Export page, Import answers and Open in browser, in one menu beside
//! the window's Hide and Close instead of a round button each.
//!
//! The page is a native CEF view that nothing drawn in this window can sit above, so the menu
//! always opens in a host window of its own (`FrostedHostKind::PageWindowMenu`) just under the ⋯
//! button: frosted like the sidebar's menus under window glass, the sidebar menu's solid fill
//! otherwise. Its rows are the sessions sidebar's (34px rows, 6px inset, 8px corners, 16px icons,
//! the shared popup menu colours), as the coordinator board's card menu builds them.
//!
//! CDXC:SessionChat 2026-11-13 DECISION:
//! User: "in the desktop's floating Ghostex page window, add a ⋯ (three dots) button next to the close and minimize buttons, and move Export page, Import answers and Open externally (open in browser) into its menu instead of separate buttons. Same menu look as other Ghostex ⋯ menus." Supersedes the round Open in browser button of the 2026-11-04 DECISION in visual_page_modal.rs and the round Export button added the same day as exported pages (server/src/visual_pages/exported.rs).
//! SEE-ALSO: apps/desktop/src/app/native_sidebar/menus.rs (the menu look), apps/desktop/src/app/native_chat/coordinator_board.rs `board_card_menu_panel` (the same rows in a host window), server/src/visual_pages/import.rs (Import answers).

use super::{GpuiVisualPageModalWindow, VisualPageModalCommand};
use crate::app::gx_store::gx_rpc;
use crate::app::native_sidebar::SIDEBAR_MENU_ROW_HEIGHT;
use crate::app::window::frosted_host::{
    FrostedHostKind, frosted_hosting_active, hide_frosted_host, show_frosted_host,
};
use gpui::{AnyElement, Bounds, Context, Pixels, Window, point, px, size};
use serde_json::json;

use super::more_menu_panel::MENU_WIDTH;
/// Between the ⋯ button's bottom and the menu's top.
const MENU_GAP: f32 = 6.0;
/// The answers an exported page gives are at most this long (gx-chat-core `page_import.rs`).
const ANSWERS_MAX_BYTES: u64 = 256 * 1024;

#[derive(Clone, Copy)]
enum MoreAction {
    Export,
    Import,
    OpenInBrowser,
}

/// The menu's actions, in the order of the panel's rows (`more_menu_panel::ROWS`).
const ROWS: [MoreAction; 3] = [
    MoreAction::Export,
    MoreAction::Import,
    MoreAction::OpenInBrowser,
];

impl GpuiVisualPageModalWindow {
    /// The ⋯ button: opens the menu under it, or closes it when it is open.
    pub(super) fn toggle_more_menu(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.more_menu_open {
            self.close_more_menu(cx);
            return;
        }
        let Some(button) = self.more_button.get() else {
            return;
        };
        let height = super::more_menu_panel::height(SIDEBAR_MENU_ROW_HEIGHT);
        let left = (button.right() - px(MENU_WIDTH)).max(px(8.0));
        let frame: Bounds<Pixels> = Bounds::new(
            point(left, button.bottom() + px(MENU_GAP)),
            size(px(MENU_WIDTH), px(height)),
        );
        self.more_menu_open = true;
        let view = cx.entity();
        show_frosted_host(
            FrostedHostKind::PageWindowMenu,
            window.window_handle(),
            frame,
            None,
            std::rc::Rc::new(move |_, _| more_menu_panel(view.clone())),
            None,
            cx,
        );
        cx.notify();
    }

    /// Takes the menu down, if it is up.
    pub(super) fn close_more_menu(&mut self, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.more_menu_open, false) {
            hide_frosted_host(FrostedHostKind::PageWindowMenu, cx);
            cx.notify();
        }
    }

    fn run_more_action(&mut self, action: MoreAction, cx: &mut Context<Self>) {
        self.close_more_menu(cx);
        match action {
            MoreAction::Export => {
                let end = self.url.find(['?', '#']).unwrap_or(self.url.len());
                let url = format!("{}/export", self.url[..end].trim_end_matches('/'));
                (self.host)(VisualPageModalCommand::ExportPage { url }, cx);
            }
            MoreAction::OpenInBrowser => {
                let url = self.url.clone();
                (self.host)(VisualPageModalCommand::OpenInBrowser { url }, cx);
            }
            MoreAction::Import => self.import_answers(cx),
        }
    }

    /// Import answers: the `.json` an exported copy of a page gave goes to the page's gxserver
    /// (`/api/importVisualPageAnswers`), which sends the agent the page's answers message and marks
    /// it answered; this page then reloads to show it.
    fn import_answers(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import answers".into()),
        });
        let remote = self.remote.clone();
        let own_page = self.page_id.clone();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let text = std::fs::metadata(&path)
                .ok()
                .filter(|metadata| metadata.len() <= ANSWERS_MAX_BYTES)
                .and_then(|_| std::fs::read_to_string(&path).ok());
            let outcome = match text {
                None => Err("The file could not be read, or is too large to be answers.".to_string()),
                Some(text) => gx_rpc(
                    remote,
                    "/api/importVisualPageAnswers",
                    json!({ "exported": text }),
                )
                .await
                .map_err(|error| {
                    if error.message.contains("projectId") {
                        "These answers are for a page this computer doesn't know. Drop the file on the chat box instead.".to_string()
                    } else {
                        error.message
                    }
                }),
            };
            let _ = this.update(cx, |this, cx| match outcome {
                Ok(result) => {
                    let title = result["title"].as_str().unwrap_or("the page").to_string();
                    let (level, message) = match result["error"].as_str() {
                        Some(error) => ("warning", format!("Answers to \"{title}\" were imported. {error}")),
                        None => ("success", format!("Answers to \"{title}\" were sent to the agent.")),
                    };
                    (this.host)(
                        VisualPageModalCommand::Notice {
                            level,
                            title: "Answers imported".to_string(),
                            message,
                        },
                        cx,
                    );
                    // This page took the answers: reload it so it shows them.
                    if own_page.is_some() && result["pageId"].as_str() == own_page.as_deref() {
                        this.reload_page(cx);
                    }
                }
                Err(message) => (this.host)(
                    VisualPageModalCommand::Notice {
                        level: "warning",
                        title: "Answers not imported".to_string(),
                        message,
                    },
                    cx,
                ),
            });
        })
        .detach();
    }
}

/// The menu's panel in the app's menu colours, filling its host window.
fn more_menu_panel(view: gpui::Entity<GpuiVisualPageModalWindow>) -> AnyElement {
    use crate::app::helpers::{
        popup_menu_surface, titlebar_popup_menu_background, titlebar_popup_menu_border_color,
        titlebar_popup_menu_foreground, titlebar_popup_menu_hover_color,
    };
    let ink = super::more_menu_panel::MenuInk {
        foreground: titlebar_popup_menu_foreground(),
        hover: titlebar_popup_menu_hover_color(),
        border: titlebar_popup_menu_border_color(),
        fill: if frosted_hosting_active() {
            popup_menu_surface()
        } else {
            titlebar_popup_menu_background()
        },
        row_height: SIDEBAR_MENU_ROW_HEIGHT,
    };
    super::more_menu_panel::panel(
        ink,
        std::rc::Rc::new(move |index, cx| {
            let Some(action) = ROWS.get(index).copied() else {
                return;
            };
            view.update(cx, |this, cx| this.run_more_action(action, cx));
        }),
    )
}
