//! Settings > Accounts' Sign in to claude.ai steps out of the way: Settings closes while claude.ai's
//! login is open in the Browser, and a small floating Back to Settings button brings it back on the
//! same account.
//!
//! CDXC:ClaudeCloud 2026-10-23 DECISION:
//! User: "when i click on sign in i think we should minimize the settings and show a floating button to show it again (go back to settings)". Sign in to claude.ai closes Settings and shows a Back to Settings button in its own small window at the bottom right of the main window, clear of claude.ai's centred login. Clicking it reopens Settings at Accounts on the same account (window/settings_modal/tabs/accounts/claude_ai_wait.rs). The button goes away when Settings is back, however it opens, or when the account's page is seen signed in.

use crate::app::window::native_modal_kit::{ModalPalette, hsla, modal_icon};
use crate::*;
use gpui::{ClickEvent, FontWeight, WindowBackgroundAppearance, WindowHandle, WindowKind};

const BUTTON_WINDOW_WIDTH: f32 = 172.0;
const BUTTON_WINDOW_HEIGHT: f32 = 44.0;
/// From the main window's bottom right corner.
const BUTTON_MARGIN: f32 = 20.0;

thread_local! {
    /// The open button, the account whose sign-in/connectors page it waits for, and the Settings
    /// tab to reopen ("accounts" or "workspaces").
    static RETURN_BUTTON: RefCell<Option<(WindowHandle<ClaudeSignInReturnButton>, String, &'static str)>> =
        const { RefCell::new(None) };
}

pub(crate) struct ClaudeSignInReturnButton {
    app: gpui::WeakEntity<GhostexGpuiApp>,
    palette: ModalPalette,
}

impl Render for ClaudeSignInReturnButton {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let p = &self.palette;
        let hover = p.raised_hover;
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .id("claude-sign-in-back-to-settings")
                    .role(gpui::Role::Button)
                    .aria_label("Back to Settings")
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .h(px(32.0))
                    .px(px(12.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .bg(hsla(p.solid_surface))
                    .hover(move |style| style.bg(hsla(hover)))
                    .cursor_pointer()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(p.foreground))
                    .child(modal_icon(
                        "modals/settings/arrow-left.svg",
                        14.0,
                        p.foreground,
                    ))
                    .child("Back to Settings")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        // This window goes with the click, so the app acts after it.
                        let app = this.app.clone();
                        cx.defer(move |cx| {
                            let _ = app.update(cx, |app, cx| {
                                app.return_to_settings_from_claude_sign_in(cx)
                            });
                        });
                    })),
            )
    }
}

/// Takes the button down (Settings is back, or the sign-in it waited for was seen).
pub(crate) fn hide_claude_sign_in_return_button(cx: &mut gpui::App) {
    if let Some((handle, _, _)) = RETURN_BUTTON.with(|button| button.borrow_mut().take()) {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
}

/// `account_id`'s claude.ai page was seen signed in: its button has nothing left to wait for when
/// it was waiting on an Accounts sign-in.
pub(crate) fn finish_claude_sign_in_return(account_id: &str, cx: &mut gpui::App) {
    let waits_for_it = RETURN_BUTTON.with(|button| {
        button
            .borrow()
            .as_ref()
            .is_some_and(|(_, waiting, tab)| waiting == account_id && *tab == "accounts")
    });
    if waits_for_it {
        hide_claude_sign_in_return_button(cx);
    }
}

impl GhostexGpuiApp {
    /// Settings' Sign in to claude.ai: Settings closes and the Back to Settings button shows.
    pub(crate) fn step_settings_aside_for_claude_sign_in(
        &mut self,
        account_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.step_settings_aside_for_claude_return(account_id, "accounts", cx);
    }

    /// Settings' claude.ai page opener: Settings closes and the Back to Settings button reopens
    /// `return_tab` ("accounts" or "workspaces").
    pub(crate) fn step_settings_aside_for_claude_return(
        &mut self,
        account_id: &str,
        return_tab: &'static str,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.native_app_modal_kind() == Some(GpuiAppModalKind::Settings) {
            self.close_native_app_modal_from_bridge(cx);
        }
        hide_claude_sign_in_return_button(cx);
        let main = self.main_window_bounds;
        let bounds = Bounds {
            origin: point(
                main.right() - px(BUTTON_WINDOW_WIDTH + BUTTON_MARGIN),
                main.bottom() - px(BUTTON_WINDOW_HEIGHT + BUTTON_MARGIN),
            ),
            size: size(px(BUTTON_WINDOW_WIDTH), px(BUTTON_WINDOW_HEIGHT)),
        };
        let app = cx.entity().downgrade();
        let palette = self.gpui_native_modal_palette();
        let main_window_native_view = self.parent_ns_view;
        // The toast window's kind of window: a pop-up that takes no focus and moves with the main
        // window.
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            display_id: self.main_window_popup_owner().display_for(bounds, cx),
            focus: false,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            titlebar: None,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        };
        let handle = cx
            .open_window(options, move |window, cx| {
                remove_gpui_app_toast_popup_window_chrome(window, main_window_native_view);
                cx.new(|_| ClaudeSignInReturnButton { app, palette })
            })
            .ok();
        if let Some(handle) = handle {
            RETURN_BUTTON.with(|button| {
                *button.borrow_mut() = Some((handle, account_id.to_string(), return_tab));
            });
        }
    }

    /// Back to Settings: reopens the Settings tab that opened claude.ai.
    pub(crate) fn return_to_settings_from_claude_sign_in(&mut self, cx: &mut gpui::Context<Self>) {
        let return_tab = RETURN_BUTTON
            .with(|button| button.borrow().as_ref().map(|(_, _, tab)| *tab))
            .unwrap_or("accounts");
        hide_claude_sign_in_return_button(cx);
        if return_tab == "accounts" {
            self.open_gpui_settings_accounts_page(None, cx);
        } else {
            self.open_gpui_settings_tab(return_tab, None, cx);
        }
    }
}
