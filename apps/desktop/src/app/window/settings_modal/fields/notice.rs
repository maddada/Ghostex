//! Two shared pieces for a Settings page with a problem to show: the empty state of a page whose
//! data comes from Ghostex's background service when that service does not answer, and a "Details"
//! fold that keeps raw technical text (an installer's output, a CLI error, a folder path) out of
//! the page until it is asked for, with a Copy button for a bug report.
//!
//! CDXC:Settings 2026-11-08 DECISION: User, on the remote tile audit page (Q2021, Q2022): "raw text
//! folds under Details with Copy details", and the pages that cannot reach the service get "one
//! shared empty state: title, one line, Try again, controls greyed out". Before, Autopilot,
//! Security and Sync each drew "Ghostex's background service is not reachable." inside a box that
//! looked like a text field, Sync with no way to retry, and Accounts, Dictation and OS Integration
//! put raw CLI and installer text in front of the reader.

use std::cell::RefCell;
use std::collections::HashSet;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Context, Entity, FontWeight, InteractiveElement as _, IntoElement as _,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    px,
};
use gpui_component::{h_flex, v_flex};

use super::super::super::native_modal_kit::*;
use super::super::palette::SettingsPalette;
use super::super::store::{SettingsStore, store_copy_to_clipboard};
use super::controls::{ButtonVariant, settings_button};
use super::row::settings_icon;

/// What a page's request reports when the service does not answer (the pages' `reload`).
pub(crate) const SERVICE_UNREACHABLE: &str = "Ghostex's background service is not reachable.";

/// The error is the service not answering, which the page shows as [`service_unreachable_state`].
pub(crate) fn is_service_unreachable(error: &str) -> bool {
    error == SERVICE_UNREACHABLE
}

/// The empty state of a page whose data comes from the service while it does not answer: an icon,
/// what is wrong, one line, and Try again.
pub(crate) fn service_unreachable_state<V: 'static>(
    p: &SettingsPalette,
    id: &'static str,
    on_retry: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    v_flex()
        .id(id)
        .w_full()
        .items_center()
        .gap(px(6.0))
        .px(px(16.0))
        .py(px(28.0))
        .rounded(px(MODAL_RADIUS_SECTION))
        .border_1()
        .border_color(hsla(p.hairline))
        .child(settings_icon("modals/settings/server.svg", 20.0, p.muted))
        .child(
            div()
                .mt(px(4.0))
                .text_size(px(14.0))
                .line_height(px(20.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(hsla(p.foreground))
                .child("Can't reach Ghostex's service"),
        )
        .child(
            div()
                .max_w(px(420.0))
                .text_center()
                .text_size(px(13.0))
                .line_height(px(19.0))
                .text_color(hsla(p.muted))
                .child(
                    "This page comes from Ghostex's background service, which isn't answering. It usually comes back by itself.",
                ),
        )
        .child(div().mt(px(8.0)).child(settings_button(
            p,
            SharedString::from(format!("{id}-retry")),
            "Try again",
            Some("modals/settings/refresh.svg"),
            ButtonVariant::Outline,
            false,
            None,
            on_retry,
            cx,
        )))
        .into_any_element()
}

thread_local! {
    /// The Details folds that are open, by id; they stay open while Settings is.
    static OPEN_DETAILS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// "Details" and "Copy" on one quiet line; the raw text shows in a monospace box under it only
/// while Details is open. `id` must be unique on the page.
pub(crate) fn raw_details(
    p: &SettingsPalette,
    id: impl Into<String>,
    raw: impl Into<String>,
    store: &Entity<SettingsStore>,
) -> AnyElement {
    let id: String = id.into();
    let raw: String = raw.into();
    let open = OPEN_DETAILS.with(|open| open.borrow().contains(&id));
    let toggle_id = id.clone();
    let copy_text = raw.clone();
    let store = store.clone();
    let link = |label: &'static str, icon: &'static str, element_id: String| {
        h_flex()
            .id(SharedString::from(element_id))
            .gap(px(4.0))
            .items_center()
            .cursor_pointer()
            .text_size(px(13.0))
            .line_height(px(18.0))
            .text_color(hsla(p.muted))
            .hover(|this| this.text_color(hsla(p.foreground)))
            .child(settings_icon(icon, 14.0, p.muted))
            .child(label)
    };
    v_flex()
        .w_full()
        .gap(px(6.0))
        .child(
            h_flex()
                .gap(px(14.0))
                .items_center()
                .child(
                    link(
                        "Details",
                        if open {
                            "modals/settings/chevron-down.svg"
                        } else {
                            "modals/settings/chevron-right.svg"
                        },
                        format!("{id}-toggle"),
                    )
                    .on_click(move |_, window, _cx| {
                        OPEN_DETAILS.with(|open| {
                            let mut open = open.borrow_mut();
                            if !open.remove(&toggle_id) {
                                open.insert(toggle_id.clone());
                            }
                        });
                        window.refresh();
                    }),
                )
                .child(
                    link(
                        "Copy details",
                        "modals/settings/copy.svg",
                        format!("{id}-copy"),
                    )
                    .on_click(move |_, _window, cx: &mut App| {
                        store_copy_to_clipboard(&store, copy_text.clone(), cx);
                    }),
                ),
        )
        .when(open, |this| {
            this.child(
                div()
                    .w_full()
                    .px(px(10.0))
                    .py(px(8.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .bg(hsla(p.foreground_alpha(0.035)))
                    .font_family(MODAL_MONO_FONT)
                    .text_size(px(12.0))
                    .line_height(px(17.0))
                    .text_color(hsla(p.muted))
                    .whitespace_normal()
                    .child(raw),
            )
        })
        .into_any_element()
}

/// A page's controls while the service does not answer: dimmed, and a click on them does nothing
/// (they could not save). The empty state above them has Try again.
pub(crate) fn greyed_out(element: AnyElement) -> AnyElement {
    div()
        .w_full()
        .opacity(0.45)
        .capture_any_mouse_down(|_, _window, cx| cx.stop_propagation())
        .capture_any_mouse_up(|_, _window, cx| cx.stop_propagation())
        .child(element)
        .into_any_element()
}
