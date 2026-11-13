//! The page window's ⋯ menu: Export page, Import answers, Open in browser and, while the page waits
//! for answers, Snooze with its lengths in a submenu, in one menu beside the window's Hide and
//! Close instead of a round button each.
//!
//! The page is a native CEF view that nothing drawn in this window can sit above, so the menu
//! always opens in a host window of its own (`FrostedHostKind::PageWindowMenu`) just under the ⋯
//! button: frosted like the sidebar's menus under window glass, the sidebar menu's solid fill
//! otherwise. Its rows are the sessions sidebar's (34px rows, 6px inset, 8px corners, 16px icons,
//! the shared popup menu colours), as the coordinator board's card menu builds them.
//!
//! CDXC:SessionChat 2026-11-13 DECISION:
//! User: "in the desktop's floating Ghostex page window, add a ⋯ (three dots) button next to the close and minimize buttons, and move Export page, Import answers and Open externally (open in browser) into its menu instead of separate buttons. Same menu look as other Ghostex ⋯ menus." Supersedes the round Open in browser button of the 2026-11-04 DECISION in visual_page_modal.rs and the round Export button added the same day as exported pages (server/src/visual_pages/exported.rs).
//! CDXC:SessionChat 2026-11-13 DECISION:
//! User: "add snooze to the inline page under the 3 dots menu (top). Also same, we need a snooze button for inline questions (Ghostex Forms)." Snooze is the dock's: For later with an end, 1, 6 or 24 hours (gx-chat-core questions/later.rs `snooze`), sent to the session that published the page (gxserver's `readVisualPageSource` names it), so the page leaves the dock and the unanswered counts until gxserver brings it back, and the agent is told nothing. Snoozing closes the window. The form card's Snooze is apps/desktop/src/app/native_chat/question_form.rs.
//! SEE-ALSO: apps/desktop/src/app/native_sidebar/menus.rs (the menu look), apps/desktop/src/app/native_chat/coordinator_board.rs `board_card_menu_panel` (the same rows in a host window), server/src/visual_pages/import.rs (Import answers).

use super::more_menu_panel::{
    MENU_WIDTH, MenuInk, SNOOZE_HOURS, SUBMENU_WIDTH, height, menu_rows, panel, snooze_rows,
};
use super::{GpuiVisualPageModalWindow, VisualPageModalCommand};
use crate::app::gx_store::gx_rpc;
use crate::app::native_sidebar::SIDEBAR_MENU_ROW_HEIGHT;
use crate::app::window::frosted_host::{
    FrostedHostKind, frosted_hosting_active, hide_frosted_host, show_frosted_host,
};
use gpui::{Bounds, Context, Pixels, Window, point, px, size};
use serde_json::json;
use std::rc::Rc;

/// Between the ⋯ button's bottom and the menu's top, and between the menu and its submenu.
const MENU_GAP: f32 = 6.0;
const SUBMENU_GAP: f32 = 4.0;
/// The answers an exported page gives are at most this long (gx-chat-core `page_import.rs`).
const ANSWERS_MAX_BYTES: u64 = 256 * 1024;

#[derive(Clone, Copy)]
enum MoreAction {
    Export,
    Import,
    OpenInBrowser,
    Snooze,
}

/// The menu's actions, in the order of the panel's rows (`more_menu_panel::menu_rows`): Snooze
/// first while the page waits for answers.
fn actions(snooze: bool) -> Vec<MoreAction> {
    let mut actions = Vec::with_capacity(4);
    if snooze {
        actions.push(MoreAction::Snooze);
    }
    actions.extend([
        MoreAction::Export,
        MoreAction::Import,
        MoreAction::OpenInBrowser,
    ]);
    actions
}

/// The open menu: where it is, its window, and whether its Snooze lengths show beside it.
#[derive(Clone, Copy)]
pub(super) struct OpenMoreMenu {
    frame: Bounds<Pixels>,
    parent: gpui::AnyWindowHandle,
    snooze: bool,
}

impl GpuiVisualPageModalWindow {
    /// Whether the menu offers Snooze: the page still waits for answers and gxserver named the
    /// session that published it.
    fn can_snooze(&self) -> bool {
        self.questions_left && self.page_session.is_some()
    }

    /// The ⋯ button: opens the menu under it, or closes it when it is open.
    pub(super) fn toggle_more_menu(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.more_menu.is_some() {
            self.close_more_menu(cx);
            return;
        }
        let Some(button) = self.more_button.get() else {
            return;
        };
        let rows = menu_rows(self.can_snooze()).len();
        let left = (button.right() - px(MENU_WIDTH)).max(px(8.0));
        let frame = Bounds::new(
            point(left, button.bottom() + px(MENU_GAP)),
            size(px(MENU_WIDTH), px(height(rows, SIDEBAR_MENU_ROW_HEIGHT))),
        );
        self.more_menu = Some(OpenMoreMenu {
            frame,
            parent: window.window_handle(),
            snooze: false,
        });
        self.show_more_menu(cx);
        cx.notify();
    }

    /// Shows the menu (and its Snooze lengths when open) in its host windows.
    fn show_more_menu(&mut self, cx: &mut Context<Self>) {
        let Some(menu) = self.more_menu else {
            return;
        };
        let snooze = self.can_snooze();
        let light = self.glass.light;
        let view = cx.entity();
        show_frosted_host(
            FrostedHostKind::PageWindowMenu,
            menu.parent,
            menu.frame,
            None,
            Rc::new(move |_, _| {
                let view = view.clone();
                panel(
                    "visual-page-more-menu",
                    menu_ink(light),
                    menu_rows(snooze),
                    menu.snooze.then_some(0),
                    Rc::new(move |index, cx| {
                        let Some(action) = actions(snooze).get(index).copied() else {
                            return;
                        };
                        view.update(cx, |this, cx| this.run_more_action(action, cx));
                    }),
                )
            }),
            None,
            cx,
        );
        if !(menu.snooze && snooze) {
            hide_frosted_host(FrostedHostKind::PageWindowSubmenu, cx);
            return;
        }
        // Beside the menu's Snooze row, on the side toward the page: the menu sits at the window's
        // right, so its lengths open to its left.
        // Snooze is the menu's first row: the lengths' first row lines up with it.
        let row_top = menu.frame.top() + px(1.0);
        let sub = Bounds::new(
            point(
                (menu.frame.left() - px(SUBMENU_WIDTH + SUBMENU_GAP)).max(px(8.0)),
                row_top,
            ),
            size(
                px(SUBMENU_WIDTH),
                px(height(SNOOZE_HOURS.len(), SIDEBAR_MENU_ROW_HEIGHT)),
            ),
        );
        let view = cx.entity();
        show_frosted_host(
            FrostedHostKind::PageWindowSubmenu,
            menu.parent,
            sub,
            None,
            Rc::new(move |_, _| {
                let view = view.clone();
                panel(
                    "visual-page-snooze-menu",
                    menu_ink(light),
                    snooze_rows(),
                    None,
                    Rc::new(move |index, cx| {
                        let Some((hours, _)) = SNOOZE_HOURS.get(index).copied() else {
                            return;
                        };
                        view.update(cx, |this, cx| this.snooze_page(hours, cx));
                    }),
                )
            }),
            None,
            cx,
        );
    }

    /// Takes the menu and its submenu down, if they are up.
    pub(super) fn close_more_menu(&mut self, cx: &mut Context<Self>) {
        if self.more_menu.take().is_some() {
            hide_frosted_host(FrostedHostKind::PageWindowMenu, cx);
            hide_frosted_host(FrostedHostKind::PageWindowSubmenu, cx);
            cx.notify();
        }
    }

    fn run_more_action(&mut self, action: MoreAction, cx: &mut Context<Self>) {
        if let MoreAction::Snooze = action {
            // The Snooze row opens its lengths on a press, and a press again folds them (no chat
            // menu opens a submenu on hover, CDXC:ContextMenus in native_chat/option_menu/render.rs).
            if let Some(menu) = self.more_menu.as_mut() {
                menu.snooze = !menu.snooze;
            }
            self.show_more_menu(cx);
            return;
        }
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
            MoreAction::Snooze => {}
        }
    }

    /// Snoozes the page for `hours` on the session that published it, then closes the window; a
    /// refusal says why and leaves the window open.
    fn snooze_page(&mut self, hours: u64, cx: &mut Context<Self>) {
        self.close_more_menu(cx);
        let (Some(page_id), Some((project_id, session_id))) =
            (self.page_id.clone(), self.page_session.clone())
        else {
            return;
        };
        let remote = self.remote.clone();
        // Saved first, so the page keeps what the reader picked for when it comes back.
        self.flush_draft(cx);
        cx.spawn(async move |this, cx| {
            let answer = gx_rpc(
                remote,
                "/api/answerSessionChatPrompt",
                json!({
                    "kind": "questionForLater",
                    "questionId": page_id,
                    "later": true,
                    "hours": hours,
                    "projectId": project_id,
                    "sessionId": session_id,
                }),
            )
            .await;
            let _ = this.update(cx, |this, cx| match answer {
                Ok(_) => (this.host)(VisualPageModalCommand::Close, cx),
                Err(error) => (this.host)(
                    VisualPageModalCommand::Notice {
                        level: "warning",
                        title: "The page was not snoozed".to_string(),
                        message: error.message,
                    },
                    cx,
                ),
            });
        })
        .detach();
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

/// The menus' colours: the page's own light-gray glass, as the window's round controls wear it
/// (`ControlGlass` in visual_page_modal.rs), white glass in light mode; solid without window glass.
///
/// CDXC:SessionChat 2026-11-13 DECISION:
/// User, of the ⋯ menu and its Snooze lengths drawn in the app's dark menu fill over the page's dusk: the user hates black on the glass design, so both take a frosted light-gray glass (white in light mode) like the page's own controls. Same day, "put snooze as top option here": Snooze is the menu's first row (`more_menu_panel::menu_rows`).
fn menu_ink(light: bool) -> MenuInk {
    let frosted = frosted_hosting_active();
    if light {
        MenuInk {
            foreground: gpui::rgb(0x18181b).into(),
            muted: gpui::rgb(0x71717b).into(),
            hover: gpui::rgba(0x0000000f).into(),
            border: gpui::rgba(0x4343432e).into(),
            fill: if frosted {
                gpui::rgba(0xffffffd6).into()
            } else {
                gpui::rgb(0xf7f7f8).into()
            },
            row_height: SIDEBAR_MENU_ROW_HEIGHT,
        }
    } else {
        MenuInk {
            foreground: gpui::rgb(0xf4f4f5).into(),
            muted: gpui::rgb(0xb4b4bb).into(),
            hover: gpui::rgba(0xffffff1c).into(),
            border: gpui::rgba(0xffffff2e).into(),
            fill: if frosted {
                gpui::rgba(0x52525acc).into()
            } else {
                gpui::rgb(0x4a4a50).into()
            },
            row_height: SIDEBAR_MENU_ROW_HEIGHT,
        }
    }
}
