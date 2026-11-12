//! The pill a hidden Ghostex page leaves in the pills bar above the chat box (pills_strip.rs), in
//! the chat whose card opened it: the page's icon, its title cut short and its answered count, at
//! a preset width. A click brings the page back as it was; pointing at it shows the toasts' round
//! x over its corner, which closes the page for good. The app owns the hidden pages
//! (app/visual_page_hidden.rs, where the user's words are) and this chat reads them through
//! `visual_popup.rs`, which the web build answers with none, since it floats no page.
//!
//! It shrinks with the bar's other pills (`plan_strip_tiers`): with its title (`PageTier::Full`)
//! while the row has room; with its icon and count only (`PageTier::Count`) once the row drops the
//! other pills' labels and still needs room, as those keep their icon and count; with its icon and
//! a dot while questions wait (`PageTier::Icon`) only once every pill shown has dropped its label;
//! then into the "+N" pill's list, like the others.

use super::{appearance::ChatAppearance, cursor::ChatCursor as _, state::NativeChatView};
use crate::app::window::round_close_button::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontFeatures, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, rgb, svg,
};
use serde_json::{Value, json};

/// A hidden page, as its pill shows it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MinimizedPage {
    /// The page window's key (`GpuiVisualPageModalWindow::key`).
    pub(crate) key: u64,
    pub(crate) title: String,
    /// A question on the page has no answer yet, or the page waits to be marked seen.
    pub(crate) questions_left: bool,
    /// `(answered, total)` of its questions, for a page that asks any.
    pub(crate) counts: Option<(u64, u64)>,
}

/// How much of a page pill shows (module docs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum PageTier {
    Full,
    Count,
    Icon,
}

/// A page pill's id in the strip's order and in the "+N" pill's list: the prefix and its key.
const PAGE_PILL_PREFIX: &str = "page:";
pub(super) const PAGE_ICON: &str = "titlebar/file-type-html.svg";

/// The preset width (unscaled) a page pill never grows past: room for its icon, about fourteen
/// characters of title and "16/16". A short title makes it narrower.
const PAGE_PILL_WIDTH: f32 = 172.0;
/// A pill's padding with its title and without (the bar's other pills', chat_pills.rs), its 1px
/// border, its icon, the gap between its parts and the dot.
const PAD_FULL: f32 = 8.0;
const PAD_BARE: f32 = 7.0;
const BORDER: f32 = 1.0;
const ICON: f32 = 14.0;
const GAP: f32 = 5.0;
const DOT: f32 = 7.0;
/// The floating x: the toasts' round x at this size, over the pill's corner by `X_OUTSET` sideways
/// and `X_RISE` up. The bar leaves 5px above a pill, so the x is never cut off at the top.
const X_SIZE: f32 = 16.0;
pub(super) const X_OUTSET: f32 = 4.0;
const X_RISE: f32 = 4.0;

/// The page key a strip id names, when it names a page pill.
pub(super) fn page_pill_key(id: &str) -> Option<u64> {
    id.strip_prefix(PAGE_PILL_PREFIX)?.parse().ok()
}

impl MinimizedPage {
    pub(super) fn pill_id(&self) -> String {
        format!("{PAGE_PILL_PREFIX}{}", self.key)
    }

    /// "3/16", for a page that asks questions.
    fn count(&self) -> Option<String> {
        self.counts
            .map(|(answered, total)| format!("{answered}/{total}"))
    }

    /// Questions on it still wait: fewer answered than asked, or gxserver says so.
    fn waiting(&self) -> bool {
        self.questions_left
            || self
                .counts
                .is_some_and(|(answered, total)| answered < total)
    }

    /// What the pill is, for its tooltip and screen readers, in the page's own words for its count
    /// ("3 of 16 answered", server/src/visual_pages/kit.rs).
    fn description(&self) -> String {
        let title = if self.title.trim().is_empty() {
            "Ghostex page"
        } else {
            self.title.trim()
        };
        match (self.counts, self.questions_left) {
            (Some((answered, total)), _) => {
                format!("Show {title} · {answered} of {total} answered")
            }
            (None, true) => format!("Show {title} · waiting for you"),
            (None, false) => format!("Show {title}"),
        }
    }

    /// The pill as a row of the "+N" pill's list (strip_overflow.rs): its title and count, or the
    /// questions' dot for a page that waits without counting.
    pub(super) fn overflow_row(&self) -> Value {
        let dots = if self.counts.is_none() && self.questions_left {
            json!([{ "tone": "question", "count": "" }])
        } else {
            json!([])
        };
        json!({
            "id": self.pill_id(),
            "label": self.title,
            "ariaLabel": self.description(),
            "count": self.count().unwrap_or_default(),
            "dots": dots,
        })
    }
}

impl NativeChatView {
    /// Reads this chat's hidden pages from the app, in a deferred call because the app entity may
    /// be mid-update while the chat renders (as `refresh_links_pill` does).
    pub(super) fn refresh_minimized_pages(&self, cx: &mut Context<Self>) {
        let Some(app) = self.config.app.clone() else {
            return;
        };
        let session = self.config.shell_session_id;
        let chat = cx.weak_entity();
        cx.defer(move |cx| {
            let Some(app) = app.upgrade() else {
                return;
            };
            let pages = super::visual_popup::minimized_pages(&app, session, cx);
            let _ = chat.update(cx, |chat, cx| {
                if chat.minimized_pages != pages {
                    chat.minimized_pages = pages;
                    cx.notify();
                }
            });
        });
    }

    /// The hidden page `id` names, when it is one of this chat's.
    pub(super) fn minimized_page(&self, id: &str) -> Option<&MinimizedPage> {
        let key = page_pill_key(id)?;
        self.minimized_pages.iter().find(|page| page.key == key)
    }

    /// A page pill's width (unscaled) at each tier, the same the pill is drawn at, so the strip
    /// plans with what it will draw (`plan_strip_tiers`).
    pub(super) fn page_pill_widths(
        &self,
        page: &MinimizedPage,
        p: &ChatAppearance,
        cx: &gpui::App,
    ) -> (f32, f32, f32) {
        let measure = |text: &str| {
            super::fonts::label_width(text, &p.font, gpui::FontWeight::NORMAL, 13.0 * p.scale, cx)
                / p.scale
        };
        // The count is in tabular figures, a touch wider than measured: one point of slack.
        let trailing = match page.count() {
            Some(count) => Some(measure(&count).ceil() + 1.0),
            None => page.questions_left.then_some(DOT),
        };
        let trailing_room = trailing.map_or(0.0, |width| GAP + width);
        let full_edge = 2.0 * (PAD_FULL + BORDER) + ICON + GAP;
        let title = measure(page.title.trim()).ceil() + 1.0;
        let full = (full_edge + title + trailing_room).min(PAGE_PILL_WIDTH);
        let bare_edge = 2.0 * (PAD_BARE + BORDER) + ICON;
        let count = bare_edge + trailing_room;
        let icon = bare_edge + if page.waiting() { GAP + DOT } else { 0.0 };
        (full, count, icon)
    }

    /// The pill at `tier`, at the width `page_pill_widths` gives it.
    pub(super) fn minimized_page_pill(
        &self,
        page: &MinimizedPage,
        tier: PageTier,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = page.key;
        let (full, count_width, icon_width) = self.page_pill_widths(page, p, cx);
        let width = match tier {
            PageTier::Full => full,
            PageTier::Count => count_width,
            PageTier::Icon => icon_width,
        };
        let description = page.description();
        let tooltip = description.clone();
        let hovered =
            self.minimized_page_hover.0 == Some(key) || self.minimized_page_hover.1 == Some(key);
        let dot = || {
            div()
                .size(px(DOT * s))
                .rounded_full()
                .flex_shrink_0()
                .bg(rgb(crate::app::native_sidebar::status::QUESTION_COLOR))
        };
        let trailing: Option<AnyElement> = match (tier, page.count()) {
            (PageTier::Icon, _) => page.waiting().then(|| dot().into_any_element()),
            (_, Some(count)) => Some(
                div()
                    .flex_shrink_0()
                    .text_color(if page.waiting() {
                        p.foreground
                    } else {
                        p.card_muted
                    })
                    .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                    .child(count)
                    .into_any_element(),
            ),
            (_, None) => page.questions_left.then(|| dot().into_any_element()),
        };
        let light = p.light;
        let close = hovered.then(|| {
            div()
                .id(("chat-page-pill-close", key))
                .absolute()
                .top(px(-X_RISE * s))
                .map(|button| {
                    if close_buttons_on_left() {
                        button.left(px(-X_OUTSET * s))
                    } else {
                        button.right(px(-X_OUTSET * s))
                    }
                })
                .size(px(X_SIZE * s))
                .role(gpui::Role::Button)
                .aria_label("Close page")
                // Over any part of the x, also what overhangs the pill, the x stays.
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    this.set_minimized_page_hover(key, true, *hovered, cx);
                }))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border_1()
                .border_color(gpui_app_toast_close_button_border_color(light))
                .bg(gpui_app_toast_close_button_color(light))
                .hover(move |style| style.bg(gpui_app_toast_close_button_hover_color(light)))
                .cursor_default()
                .tooltip(|window, cx| {
                    gpui_component::tooltip::Tooltip::new("Close page").build(window, cx)
                })
                .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.minimized_page_hover = (None, None);
                    this.host("closeMinimizedPage", json!({ "key": key }), cx);
                }))
                .child(round_close_x_icon(
                    X_SIZE * s / ROUND_CLOSE_BUTTON_SIZE,
                    gpui_app_toast_close_button_icon_color(light),
                ))
        });
        div()
            .id(("chat-page-pill", key))
            .relative()
            .role(gpui::Role::Button)
            .aria_label(description)
            .flex()
            .items_center()
            .flex_shrink_0()
            .w(px(width * s))
            .gap(px(GAP * s))
            .px(px(if tier == PageTier::Full {
                PAD_FULL
            } else {
                PAD_BARE
            } * s))
            .py(px(2.0 * s))
            .rounded_full()
            .border(px(s))
            .border_color(p.foreground.opacity(0.1))
            .text_size(px(13.0 * s))
            .line_height(px(19.0 * s))
            .whitespace_nowrap()
            .text_color(p.card_muted)
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.06)))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            })
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.set_minimized_page_hover(key, false, *hovered, cx);
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.minimized_page_hover = (None, None);
                this.host("showMinimizedPage", json!({ "key": key }), cx);
            }))
            .child(
                svg()
                    .path(PAGE_ICON)
                    .size(px(ICON * s))
                    .flex_shrink_0()
                    .text_color(p.card_muted),
            )
            .when(tier == PageTier::Full, |pill| {
                pill.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_color(p.foreground)
                        .child(page.title.trim().to_string()),
                )
            })
            .children(trailing)
            .children(close)
            .into_any_element()
    }

    /// The pointer is over a page pill (`x` false) or its floating x (`x` true). The x shows while
    /// either is, so the pill and its x are one hover area whichever hears of a move first.
    fn set_minimized_page_hover(
        &mut self,
        key: u64,
        x: bool,
        hovered: bool,
        cx: &mut Context<Self>,
    ) {
        let slot = if x {
            &mut self.minimized_page_hover.1
        } else {
            &mut self.minimized_page_hover.0
        };
        let next = if hovered {
            Some(key)
        } else if *slot == Some(key) {
            None
        } else {
            return;
        };
        if *slot != next {
            *slot = next;
            cx.notify();
        }
    }
}
