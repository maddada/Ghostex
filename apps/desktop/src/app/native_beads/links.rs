//! The details' Links row: the bead's external link and spec link (bd's `external_ref` and
//! `spec_id`), opened with a click and set or cleared from here where the computer supports it.
//!
//! CDXC:ProjectBoard 2026-11-13 DECISION: User (settled in the Beads view fixes plan): bd keeps no attachments, so the view offers links, not files. A computer whose Ghostex predates link editing (no `links` capability) shows its links read-only with an update hint, so a save never fails there.

use ghostex_gx_protocol::beads_view::capability;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::input::{InputEvent, InputState};
use serde_json::{Value, json};

use super::view::BeadsView;
use crate::app::helpers::titlebar_svg_icon;
use crate::app::native_kanban::palette::KanbanPalette;
use crate::app::native_kanban::widgets::text_field;

/// Which of a bead's two links.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LinkKind {
    External,
    Spec,
}

impl LinkKind {
    /// The `update` field gxserver takes.
    fn param(self) -> &'static str {
        match self {
            LinkKind::External => "externalRef",
            LinkKind::Spec => "specId",
        }
    }

    /// The field bd's `show` answer carries.
    fn shown_key(self) -> &'static str {
        match self {
            LinkKind::External => "external_ref",
            LinkKind::Spec => "spec_id",
        }
    }

    fn add_label(self) -> &'static str {
        match self {
            LinkKind::External => "Add link",
            LinkKind::Spec => "Add spec",
        }
    }

    fn placeholder(self) -> &'static str {
        match self {
            LinkKind::External => "Link or issue, then Enter",
            LinkKind::Spec => "Spec link or path, then Enter",
        }
    }

    fn slug(self) -> &'static str {
        match self {
            LinkKind::External => "external",
            LinkKind::Spec => "spec",
        }
    }
}

/// A link that opens in the browser; anything else (an issue key, a path) is shown as text.
fn openable(link: &str) -> bool {
    link.starts_with("https://") || link.starts_with("http://")
}

/// A long URL shortened for the chip: host and the end of the path.
fn chip_text(link: &str) -> String {
    let bare = link
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    if bare.chars().count() <= 40 {
        return bare.to_string();
    }
    let head: String = bare.chars().take(18).collect();
    let tail: String = bare
        .chars()
        .rev()
        .take(18)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{head}…{tail}")
}

impl BeadsView {
    pub(crate) fn can_edit_links(&self) -> bool {
        self.has_capability(capability::LINKS)
    }

    /// Opens the input for one link.
    pub(crate) fn start_link(
        &mut self,
        kind: LinkKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(details) = self.details.as_mut() else {
            return;
        };
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(kind.placeholder()));
        details._subscriptions.push(cx.subscribe_in(
            &input,
            window,
            move |this: &mut Self, input, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let link = input.read(cx).value().trim().to_string();
                    if !link.is_empty() {
                        this.set_link(kind, link, window, cx);
                    }
                }
            },
        ));
        input.update(cx, |input, cx| input.focus(window, cx));
        details.link = Some((kind, input));
        cx.notify();
    }

    /// Sets (or, with an empty `link`, clears) one link, then reads the bead again.
    fn set_link(
        &mut self,
        kind: LinkKind,
        link: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.details.as_mut().map(|details| {
            details.link = None;
            details.id.clone()
        }) else {
            return;
        };
        let mut params = json!({ "action": "update", "issueId": id });
        params[kind.param()] = json!(link);
        self.run_action(
            params,
            "Couldn't change the link",
            window,
            cx,
            |this, _, _, cx| this.load_details(cx),
        );
    }

    /// The Links row's value; `None` when there is nothing to show (no links and no editing).
    pub(crate) fn render_links(
        &self,
        shown: Option<&Value>,
        p: &KanbanPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let editable = self.can_edit_links();
        let editing = self
            .details
            .as_ref()
            .and_then(|details| details.link.clone());
        let links: Vec<(LinkKind, String)> = [LinkKind::External, LinkKind::Spec]
            .into_iter()
            .filter_map(|kind| {
                shown
                    .and_then(|shown| shown.get(kind.shown_key()))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|link| !link.is_empty())
                    .map(|link| (kind, link.to_string()))
            })
            .collect();
        if links.is_empty() && !editable {
            return None;
        }
        let mut row = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.0))
            .text_size(px(12.0));
        for (kind, link) in &links {
            let (kind, link) = (*kind, link.clone());
            let open = link.clone();
            let full = link.clone();
            // The chip shrinks to the row's width and cuts its text with an ellipsis, so the ✕
            // stays inside the panel however long the link is; clicking still opens the whole link.
            let mut chip = div()
                .id(SharedString::from(format!("beads-link-{}", kind.slug())))
                .flex()
                .max_w_full()
                .min_w_0()
                .items_center()
                .gap(px(4.0))
                .px(px(6.0))
                .h(px(20.0))
                .rounded(px(5.0))
                .bg(p.control)
                .text_color(p.muted)
                .child(titlebar_svg_icon(
                    if kind == LinkKind::Spec {
                        "titlebar/file-text.svg"
                    } else {
                        "titlebar/link.svg"
                    },
                    11.0,
                    p.faint,
                ))
                // The chip may shorten the link; hovering shows it whole.
                .tooltip(move |window, cx| {
                    crate::app::helpers::titlebar_tooltip(full.clone(), window, cx)
                })
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(chip_text(&link)),
                );
            if openable(&link) {
                chip = chip
                    .cursor_pointer()
                    .hover(|this| this.text_color(p.foreground))
                    .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&open)));
            }
            if editable {
                chip = chip.child(
                    div()
                        .id(SharedString::from(format!("beads-link-x-{}", kind.slug())))
                        .flex_none()
                        .cursor_pointer()
                        .child(titlebar_svg_icon("titlebar/x.svg", 11.0, p.faint))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.set_link(kind, String::new(), window, cx)
                        })),
                );
            }
            row = row.child(chip);
        }
        if let Some((_, input)) = editing.as_ref() {
            row = row.child(
                div()
                    .w_full()
                    .min_w_0()
                    .child(text_field(input, p, window, cx)),
            );
        } else if editable {
            for kind in [LinkKind::External, LinkKind::Spec] {
                if links.iter().any(|(have, _)| *have == kind) {
                    continue;
                }
                row = row.child(
                    div()
                        .id(SharedString::from(format!("beads-add-{}", kind.slug())))
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .px(px(8.0))
                        .h(px(22.0))
                        .rounded(px(6.0))
                        .border_1()
                        .border_dashed()
                        .border_color(p.border_strong)
                        .text_color(p.muted)
                        .cursor_pointer()
                        .child(titlebar_svg_icon("titlebar/plus.svg", 11.0, p.muted))
                        .child(kind.add_label())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.start_link(kind, window, cx)
                        })),
                );
            }
        }
        let row = row.when(!editable, |this| {
            this.child(
                div()
                    .w_full()
                    .text_color(p.faint)
                    .child("Update Ghostex on that computer to edit links."),
            )
        });
        Some(row.into_any_element())
    }
}
