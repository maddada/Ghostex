//! A page's question about the computer (open another app, connect to apps on this computer),
//! drawn at the top of that page. `app/browser_site_requests.rs` decides what to ask and what the
//! answer does; this file queues the questions per page, draws them and takes the answer.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

use futures::channel::oneshot;
use gpui::{KeyDownEvent, MouseDownEvent, WeakEntity};
use gpui_component::ActiveTheme as _;

use crate::*;

thread_local! {
    /// Every CEF page's surface by its browser id, so a question reaches the page that asked.
    static SURFACES_BY_BROWSER: RefCell<HashMap<i32, WeakEntity<CefSurface>>> =
        RefCell::new(HashMap::new());
}

pub(crate) fn register_site_prompt_surface(browser_id: i32, surface: WeakEntity<CefSurface>) {
    SURFACES_BY_BROWSER.with(|surfaces| surfaces.borrow_mut().insert(browser_id, surface));
}

pub(crate) fn unregister_site_prompt_surface(browser_id: i32, surface: gpui::EntityId) {
    SURFACES_BY_BROWSER.with(|surfaces| {
        let mut surfaces = surfaces.borrow_mut();
        if surfaces
            .get(&browser_id)
            .is_some_and(|registered| registered.entity_id() == surface)
        {
            surfaces.remove(&browser_id);
        }
    });
}

/// The page a CEF browser draws in, None when it has none any more.
pub(crate) fn cef_surface_for_browser(browser_id: i32) -> Option<WeakEntity<CefSurface>> {
    SURFACES_BY_BROWSER.with(|surfaces| surfaces.borrow().get(&browser_id).cloned())
}

#[derive(Clone, Copy)]
pub(crate) enum SitePromptIcon {
    ExternalApp,
    LocalNetwork,
}

pub(crate) struct SitePromptContent {
    /// Questions with the same key are not queued twice on one page.
    pub(crate) key: String,
    pub(crate) icon: SitePromptIcon,
    pub(crate) message: String,
    pub(crate) detail: Option<String>,
    pub(crate) allow_label: String,
    pub(crate) deny_label: String,
}

struct SitePrompt {
    id: u64,
    content: SitePromptContent,
    /// Allow (true) or Don't Allow (false). Dropping it unanswered is "not now".
    answer: Option<oneshot::Sender<bool>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SitePromptButton {
    Deny,
    Allow,
    Close,
}

/// The questions waiting on one page, the front one shown.
pub(crate) struct SitePromptQueue {
    prompts: VecDeque<SitePrompt>,
    next_id: u64,
    row_focus: FocusHandle,
    deny_focus: FocusHandle,
    allow_focus: FocusHandle,
    close_focus: FocusHandle,
    /// The question the keyboard decision was made for, so it is made once per question.
    presented: Option<u64>,
    /// The question row took the keyboard from the page, which gets it back when the row goes.
    holds_focus: bool,
}

impl SitePromptQueue {
    pub(crate) fn new(cx: &mut App) -> Self {
        Self {
            prompts: VecDeque::new(),
            next_id: 1,
            row_focus: cx.focus_handle(),
            deny_focus: cx.focus_handle(),
            allow_focus: cx.focus_handle(),
            close_focus: cx.focus_handle(),
            presented: None,
            holds_focus: false,
        }
    }

    fn focused_button(&self, window: &Window) -> Option<SitePromptButton> {
        [
            (SitePromptButton::Deny, &self.deny_focus),
            (SitePromptButton::Allow, &self.allow_focus),
            (SitePromptButton::Close, &self.close_focus),
        ]
        .into_iter()
        .find(|(_, handle)| handle.is_focused(window))
        .map(|(button, _)| button)
    }

    fn button_focus(&self, button: SitePromptButton) -> &FocusHandle {
        match button {
            SitePromptButton::Deny => &self.deny_focus,
            SitePromptButton::Allow => &self.allow_focus,
            SitePromptButton::Close => &self.close_focus,
        }
    }

    fn owns_focus(&self, window: &Window) -> bool {
        self.row_focus.is_focused(window) || self.focused_button(window).is_some()
    }
}

/// Moves the keyboard from the page's native view to the question.
fn focus_site_prompt(handle: &FocusHandle, window: &mut Window, cx: &mut App) {
    if let Ok(parent) = cef_parent_native_view(window) {
        crate::cef::focus_gpui_root_view(parent);
    }
    handle.focus(window, cx);
}

impl CefSurface {
    /// Queues a question on this page. None when the page already has the same question waiting;
    /// otherwise the question's id and its answer, which resolves to Err for "not now" (the ×,
    /// Escape, the page or its tab gone).
    pub(crate) fn push_site_prompt(
        &mut self,
        content: SitePromptContent,
        cx: &mut gpui::Context<Self>,
    ) -> Option<(u64, oneshot::Receiver<bool>)> {
        let queue = &mut self.site_prompts;
        if queue
            .prompts
            .iter()
            .any(|prompt| prompt.content.key == content.key)
        {
            return None;
        }
        let id = queue.next_id;
        queue.next_id += 1;
        let (sender, receiver) = oneshot::channel();
        queue.prompts.push_back(SitePrompt {
            id,
            content,
            answer: Some(sender),
        });
        cx.notify();
        Some((id, receiver))
    }

    /// Takes a question down unanswered ("not now"), as when its page went away.
    pub(crate) fn remove_site_prompt(&mut self, id: u64, cx: &mut gpui::Context<Self>) {
        let queue = &mut self.site_prompts;
        let before = queue.prompts.len();
        queue.prompts.retain(|prompt| prompt.id != id);
        if queue.prompts.len() != before {
            cx.notify();
        }
    }

    fn answer_site_prompt(&mut self, answer: Option<bool>, cx: &mut gpui::Context<Self>) {
        let Some(mut prompt) = self.site_prompts.prompts.pop_front() else {
            return;
        };
        if let (Some(answer), Some(sender)) = (answer, prompt.answer.take()) {
            let _ = sender.send(answer);
        }
        cx.notify();
    }

    fn handle_site_prompt_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let focused = self.site_prompts.focused_button(window);
        match event.keystroke.key.as_str() {
            "escape" => self.answer_site_prompt(None, cx),
            "enter" => match focused {
                Some(SitePromptButton::Allow) => self.answer_site_prompt(Some(true), cx),
                Some(SitePromptButton::Deny) => self.answer_site_prompt(Some(false), cx),
                Some(SitePromptButton::Close) => self.answer_site_prompt(None, cx),
                // The row itself: Enter picks nothing, so a page cannot have a keystroke meant
                // for it answer its own question.
                None => return,
            },
            "tab" => {
                let order = [
                    SitePromptButton::Deny,
                    SitePromptButton::Allow,
                    SitePromptButton::Close,
                ];
                let index = focused.and_then(|button| order.iter().position(|b| *b == button));
                let next = match (index, event.keystroke.modifiers.shift) {
                    (None, false) => 0,
                    (None, true) => order.len() - 1,
                    (Some(index), false) => (index + 1) % order.len(),
                    (Some(index), true) => (index + order.len() - 1) % order.len(),
                };
                let handle = self.site_prompts.button_focus(order[next]).clone();
                focus_site_prompt(&handle, window, cx);
                cx.notify();
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    /// CDXC:Browser 2026-10-10 WHY:
    /// The question was a modal OS dialog owned by the window, so Windows disabled the window
    /// (`IsWindowEnabled = False`) and its close button did nothing until the user answered (live
    /// test, ShortPoint window). Like Chrome's permission bubble it now never blocks: it is a row
    /// at the top of the page that asked, under a Browser pane's toolbar, laid out like the
    /// microphone/camera prompt so it shrinks the page instead of covering it (a popup window
    /// would have to chase the pane through window moves, tab switches and panel slides, and the
    /// page's native view draws over anything GPUI paints inside it). Same on macOS, where it was
    /// a sheet. Questions queue per page and show one at a time; the × and Escape are "not now"
    /// and store nothing. The row takes the keyboard only when the page that asked had it, and
    /// focuses no button, so Enter typed for the page cannot answer; Tab reaches the buttons.
    pub(crate) fn render_site_prompt_row(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let Some(front_id) = self.site_prompts.prompts.front().map(|prompt| prompt.id) else {
            if self.site_prompts.holds_focus {
                self.site_prompts.holds_focus = false;
                self.site_prompts.presented = None;
                if self.site_prompts.owns_focus(window) {
                    cx.defer_in(window, |surface, window, cx| {
                        surface.focus_handle.focus(window, cx);
                        surface.browser.focus();
                    });
                }
            }
            return None;
        };
        if !self.visible {
            return None;
        }
        if self.site_prompts.presented != Some(front_id) {
            self.site_prompts.presented = Some(front_id);
            let page_has_keyboard = window.is_window_active()
                && (self.focus_handle.is_focused(window) || self.browser.owns_native_focus());
            if page_has_keyboard && !self.site_prompts.holds_focus {
                self.site_prompts.holds_focus = true;
                let row_focus = self.site_prompts.row_focus.clone();
                cx.defer_in(window, move |_, window, cx| {
                    focus_site_prompt(&row_focus, window, cx);
                });
            }
        }
        let prompt = self.site_prompts.prompts.front()?;
        let content = &prompt.content;
        let id_suffix = format!("{}-{}", self.id, prompt.id);
        let icon = match content.icon {
            SitePromptIcon::ExternalApp => TITLEBAR_ICON_EXTERNAL_LINK,
            SitePromptIcon::LocalNetwork => BROWSER_ICON_LOCK_FILLED,
        };
        let ring = cx.theme().ring;
        let focused = self.site_prompts.focused_button(window);
        let button = |button: SitePromptButton,
                      label: String,
                      primary: bool,
                      cx: &mut gpui::Context<Self>| {
            let (background, hover_background, border, text) = if primary {
                (0.16, 0.22, 0.28, 0.95)
            } else {
                (0.06, 0.11, 0.16, 0.8)
            };
            let answer = match button {
                SitePromptButton::Allow => Some(true),
                SitePromptButton::Deny => Some(false),
                SitePromptButton::Close => None,
            };
            let base = div()
                .id(format!(
                    "ghostex-gpui-site-prompt-{}-{id_suffix}",
                    match button {
                        SitePromptButton::Allow => "allow",
                        SitePromptButton::Deny => "deny",
                        SitePromptButton::Close => "close",
                    }
                ))
                .track_focus(self.site_prompts.button_focus(button))
                .flex()
                .flex_shrink_0()
                .h(px(24.0))
                .items_center()
                .justify_center()
                .rounded(px(5.0))
                .border_1()
                .border_color(if focused == Some(button) {
                    ring
                } else {
                    chrome_ink().opacity(border).into()
                })
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                    }),
                )
                .on_click(cx.listener(move |surface, _, _, cx| {
                    surface.answer_site_prompt(answer, cx);
                }));
            match button {
                SitePromptButton::Close => base
                    .w(px(24.0))
                    .border_color(if focused == Some(button) {
                        ring
                    } else {
                        gpui::transparent_black()
                    })
                    .hover(|this| this.bg(chrome_ink().opacity(0.08)))
                    .child(titlebar_svg_icon(
                        TITLEBAR_ICON_X,
                        13.0,
                        browser_toolbar_button_icon_color(),
                    ))
                    .into_any_element(),
                _ => base
                    .bg(chrome_ink().opacity(background))
                    .px(px(12.0))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(chrome_ink().opacity(text))
                    .hover(|this| this.bg(chrome_ink().opacity(hover_background)))
                    .child(label)
                    .into_any_element(),
            }
        };
        let deny = button(
            SitePromptButton::Deny,
            content.deny_label.clone(),
            false,
            cx,
        );
        let allow = button(
            SitePromptButton::Allow,
            content.allow_label.clone(),
            true,
            cx,
        );
        let close = button(SitePromptButton::Close, String::new(), false, cx);
        let row_focus = self.site_prompts.row_focus.clone();
        Some(
            div()
                .id(format!("ghostex-gpui-site-prompt-row-{id_suffix}"))
                .track_focus(&self.site_prompts.row_focus)
                .on_key_down(cx.listener(Self::handle_site_prompt_key))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |surface, _: &MouseDownEvent, window, cx| {
                        surface.site_prompts.holds_focus = true;
                        if !surface.site_prompts.owns_focus(window) {
                            focus_site_prompt(&row_focus, window, cx);
                        }
                    }),
                )
                .flex()
                .flex_row()
                .flex_shrink_0()
                .w_full()
                .min_h(px(BROWSER_MEDIA_PERMISSION_BAR_HEIGHT))
                .py(px(6.0))
                .items_center()
                .gap(px(8.0))
                .px(px(BROWSER_TOOLBAR_HORIZONTAL_PADDING))
                .bg(browser_toolbar_background())
                .border_b_1()
                .border_color(chrome_color(0x252525, 0xd4d4d4))
                .child(titlebar_svg_icon(
                    icon,
                    15.0,
                    browser_toolbar_button_icon_color(),
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(chrome_ink().opacity(0.92))
                                .child(content.message.clone()),
                        )
                        .when_some(content.detail.clone(), |this, detail| {
                            this.child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(chrome_ink().opacity(0.62))
                                    .child(detail),
                            )
                        }),
                )
                .child(deny)
                .child(allow)
                .child(close)
                .into_any_element(),
        )
    }
}
