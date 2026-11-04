use super::super::{
    appearance::{ChatAppearance, ChatFilledButton as _},
    state::NativeChatView,
};
use super::layout::SPEC;
use super::window::SuggestionPanel;
use crate::app::helpers::ThrottledAnimationExt as _;
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Window, div, point, px, rgb,
};
use serde_json::json;

impl Render for SuggestionPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.chat.read(cx).snapshot.clone();
        let inset = self
            .windowed
            .then(|| self.chat.read(cx).suggestions.card_in_window())
            .flatten();
        // Under window glass the child window blurs what is behind it (window.rs), so the card and
        // its rows take the frosted treatment; the maximized composer's inline panel stays opaque.
        let glass =
            self.windowed && crate::app::helpers::window_glass_active_for(Some(self.source));
        let p = ChatAppearance::current(&state).on_window_glass(glass);
        let s = p.scale;
        let spec = &*SPEC;
        let data = &state["suggestions"];
        let rows = data["rows"].as_array().cloned().unwrap_or_default();
        let selected = data["selected"].as_u64().unwrap_or(0) as usize;
        let (hover, echo) = {
            let list = &self.chat.read(cx).suggestions;
            (list.hover, list.hovered.contains(&selected))
        };
        if self.selected != Some(selected) {
            self.selected = Some(selected);
            /*
            CDXC:SessionChat 2026-10-06 WHY:
            The core's answer to a hover arrives a frame or more after the pointer moved, and scrolling to every new selection pulled the list back to the row hovered before the wheel moved it on, so scrolling while hovering fought the wheel. Only a selection the keyboard or an edit made scrolls into view.
            */
            if !echo {
                self.scroll
                    .scroll_to_item(selected + 1 + usize::from(data["status"].is_string()));
            }
        }
        let selected = hover.filter(|row| *row < rows.len()).unwrap_or(selected);
        let files = data["kind"] == "file";
        // The `+` list (gx-chat-core `transcript/session_mentions.rs`): each row leads with its
        // agent's logo, and a working session's state reads in the working amber.
        let sessions = data["kind"] == "session";
        /*
        CDXC:SessionChat 2026-09-23 DECISION:
        User: the `$`, `@` and `/` list "looks out of place" above the frosted composer ("can we make this look more fitting please?"): a heavy near-black card whose rows each carried their own outline, so the list read as separate black blocks with thick seams. Under window glass the card is the chat's frosted menu surface in a blurred window, its edge the composer's soft ink border, and the rows are one continuous list: no per-row outline, and the selected row is a wash of the menu's ink. Row corners still follow the 2026-09-19 rule (only the list's outer corners round). The opaque card keeps React's `bg-popover`, `bg-accent` and the legacy row outline.
        */
        let popover = if glass {
            p.menu_surface()
        } else {
            p.menu_opaque()
        };
        let highlight: Hsla = if glass {
            Hsla::from(rgb(if p.light { 0x000000 } else { 0xffffff })).opacity(if p.light {
                0.06
            } else {
                0.08
            })
        } else {
            rgb(if p.light { 0xf4f4f5 } else { 0x333333 }).into()
        };
        let outline = if glass {
            gpui::transparent_black()
        } else {
            row_outline()
        };
        let card_border = if glass { p.border } else { p.input_border };
        let inline = px(spec.padding_inline_px * s);
        let gap = px(spec.row_gap_px * s);
        let card_radius = spec.card_radius(s, glass);
        let row_radius = spec.row_radius(card_radius, s);
        let heading = data["heading"].as_str().unwrap_or_default().to_uppercase();
        let mut body = div()
            .id("suggestion-list")
            .role(gpui::Role::ListBox)
            .aria_label(match data["kind"].as_str() {
                Some("slash") => "Slash commands",
                Some("skill") => "Available skills",
                Some("session") => "Agents in this project",
                _ => "Project files",
            })
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .p(px(spec.list_padding_px * s))
            .child(
                // GPUI text has no letter spacing, so React's `tracking-[0.14em]` is laid out one
                // glyph at a time, each followed by the same advance CSS adds after every letter.
                div()
                    .id("suggestion-heading")
                    .role(gpui::Role::Heading)
                    .aria_label(heading.clone())
                    .flex()
                    .overflow_hidden()
                    .px(inline)
                    .pt(px(spec.heading_padding_top_px * s))
                    .pb(px(spec.heading_padding_bottom_px * s))
                    .text_size(px(spec.heading_font_size_px * s))
                    .line_height(px(spec.heading_line_height_px * s))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(p.muted)
                    .children(heading.chars().map(|glyph| {
                        div()
                            .flex_shrink_0()
                            .mr(px(spec.heading_letter_spacing_px * s))
                            .child(glyph.to_string())
                    })),
            );
        if let Some(status) = data["status"].as_str() {
            let retry = data["retry"] == true;
            body = body.child(
                div()
                    .id("suggestion-status")
                    .role(gpui::Role::Status)
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(gap)
                    .h(spec.status_height(retry, s))
                    .px(inline)
                    // React dimmed the loading and empty rows; the error keeps the popup's text colour.
                    .when(!retry, |row| row.text_color(p.muted))
                    .when(data["loading"] == true, |row| {
                        row.child(suggestion_spinner(px(spec.icon_px * s), p.muted))
                    })
                    .child(div().flex_1().min_w_0().truncate().child(status.to_owned()))
                    .when(retry, |row| {
                        // React's `<Button size="sm" variant="outline">`, now the neutral fill
                        // (CDXC:AppModal 2026-10-14 on `neutral_button_fills`).
                        row.child(
                            div()
                                .id("retry-skills")
                                .role(gpui::Role::Button)
                                .aria_label("Retry")
                                .chat_cursor_pointer()
                                .flex()
                                .flex_shrink_0()
                                .items_center()
                                .h(px(spec.retry_height_px * s))
                                .px(px(spec.retry_padding_inline_px * s))
                                .rounded(px(spec.retry_radius_px * s))
                                .border(px(spec.border_px))
                                .chat_filled(p.neutral_button_fills(), true)
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .child("Retry")
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(|this, _, window, cx| {
                                        window.prevent_default();
                                        cx.stop_propagation();
                                        this.choose(json!({"type":"suggestionRetry"}), cx);
                                    }),
                                ),
                        )
                    }),
            );
        }
        for (index, row) in rows.into_iter().enumerate() {
            let label = row["label"].as_str().unwrap_or_default().to_owned();
            let detail = row["detail"].as_str().unwrap_or_default().to_owned();
            // Only the list's outer corners are rounded (sessionChatSuggestionRowCorners).
            let (round_top, round_bottom) = (row["roundTop"] == true, row["roundBottom"] == true);
            let logo = sessions.then(|| {
                super::super::markdown_links::session_pill_icon(
                    row["agentIcon"].as_str().unwrap_or_default(),
                    row["coordinator"] == true,
                    &p,
                )
            });
            let working = row["working"] == true;
            body = body.child(
                div()
                    .id(("suggestion", index))
                    .role(gpui::Role::ListBoxOption)
                    .aria_selected(index == selected)
                    .aria_label(format!("{label} {detail}"))
                    .flex()
                    .items_center()
                    .gap(gap)
                    .w_full()
                    .min_w_0()
                    .h(spec.row_height(s))
                    .px(inline)
                    .when(round_top, |row| row.rounded_t(row_radius))
                    .when(round_bottom, |row| row.rounded_b(row_radius))
                    .border(px(spec.row_border_px))
                    .border_color(outline)
                    .chat_cursor_pointer()
                    .when(index == selected, |row| row.bg(highlight))
                    .when(files, |row| {
                        row.child(
                            gpui::svg()
                                .path("titlebar/file.svg")
                                .size(px(spec.icon_px * s))
                                .flex_shrink_0()
                                .text_color(p.muted),
                        )
                    })
                    .when_some(logo, |row, (logo, color)| {
                        row.child(
                            gpui::svg()
                                .path(logo)
                                .size(px(spec.icon_px * s))
                                .flex_shrink_0()
                                .text_color(color),
                        )
                    })
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .map(|text| {
                                if sessions {
                                    text.flex_1()
                                } else {
                                    text.flex_shrink_0()
                                }
                            })
                            // A long name (`/plugin:skill`) widens its own row up to the phone's 60%.
                            .when(!files && !sessions, |text| {
                                text.min_w(px(spec.label_column_px * s))
                                    .max_w(gpui::relative(0.6))
                            })
                            .child(label),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .when(sessions, |text| text.flex_none().max_w(px(160.0 * s)))
                            .text_color(if working {
                                rgb(super::super::chat_pills::ATTENTION_COLOR).into()
                            } else {
                                p.muted
                            })
                            .child(detail),
                    )
                    .on_mouse_move(cx.listener(move |this, _, _, cx| {
                        let core = this.selected;
                        let moved = this.chat.update(cx, |chat, cx| {
                            let list = &mut chat.suggestions;
                            if list.hover == Some(index)
                                || list.hover.is_none() && core == Some(index)
                            {
                                return false;
                            }
                            list.hover = Some(index);
                            if !list.hovered.contains(&index) {
                                list.hovered.push(index);
                            }
                            chat.invoke(json!({"type":"suggestionHighlight","index":index}), cx);
                            true
                        });
                        if moved {
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.choose(json!({"type":"suggestionPick","index":index}), cx);
                        }),
                    ),
            );
        }
        let card = div()
            .rounded(card_radius)
            .border(px(spec.border_px))
            .border_color(card_border)
            .bg(popover)
            .text_color(p.foreground)
            .font_family(p.font)
            .text_size(px(spec.row_font_size_px * s))
            .line_height(px(spec.row_line_height_px * s))
            .overflow_hidden()
            .child(body);
        match inset {
            Some(inset) => div()
                .size_full()
                .child(
                    card.absolute()
                        .left(inset.left())
                        .top(inset.top())
                        .w(inset.size.width)
                        .h(inset.size.height),
                )
                .into_any_element(),
            None => card.size_full().into_any_element(),
        }
    }
}

impl NativeChatView {
    /// CDXC:SessionChat 2026-09-19 WHY:
    /// React's list cast `shadow-xl` over the transcript and the top of the composer card. The popup's child window cannot paint it: the window would have to grow past the card, and that transparent margin would sit over the composer and the transcript and take their clicks, while the macOS window shadow outlines the card with a dark rim instead. The pane paints the shadow at the popup's frame as plain chrome with no hit area, and the popup's opaque card covers the part beneath it.
    pub(in crate::app::native_chat) fn render_suggestion_shadow(
        &self,
        p: &ChatAppearance,
    ) -> Option<AnyElement> {
        let card = self.suggestions.bounds?;
        let pane = self.bounds.get().origin;
        let s = p.scale;
        let spec = &*SPEC;
        let glass = crate::app::helpers::window_glass_active_for(self.main_window);
        Some(
            div()
                .absolute()
                .left(card.left() - pane.x)
                .top(card.top() - pane.y)
                .w(card.size.width)
                .h(card.size.height)
                .rounded(spec.card_radius(s, glass))
                .shadow(
                    spec.shadows
                        .iter()
                        .map(|layer| gpui::BoxShadow {
                            color: gpui::hsla(0.0, 0.0, 0.0, layer.alpha),
                            offset: point(px(0.0), px(layer.offset_y_px * s)),
                            // GPUI's blur radius is the Gaussian's sigma, CSS's is twice that.
                            blur_radius: px(layer.blur_px * s / 2.0),
                            spread_radius: px(layer.spread_px * s),
                            inset: false,
                        })
                        .collect(),
                )
                .into_any_element(),
        )
    }
}

/// Every React row was a bare `<button>`, so theme.css's legacy base outlined it with the app
/// theme's `--app-border`, whatever the chat's own theme: black at 12% under the plain light app
/// theme and white at 11% under every other one.
fn row_outline() -> Hsla {
    let snapshot = crate::shared_settings::shared_sidebar_settings_snapshot();
    if crate::app::helpers::gpui_app_modal_sidebar_theme_from_settings(snapshot.object())
        == "plain-light"
    {
        Hsla::from(rgb(0x000000)).opacity(0.12)
    } else {
        Hsla::from(rgb(0xffffff)).opacity(0.11)
    }
}

/// React's `IconLoader2` beside "Loading skills…" and "Listing project files…", still when the
/// system asks for reduced motion.
fn suggestion_spinner(size: gpui::Pixels, color: gpui::Hsla) -> gpui::AnyElement {
    let glyph = gpui::svg()
        .path("titlebar/loader2.svg")
        .size(size)
        .flex_shrink_0()
        .text_color(color);
    if crate::app::helpers::gpui_macos_reduce_motion_enabled() {
        return glyph.into_any_element();
    }
    glyph
        .with_throttled_animation(
            "suggestion-loading-spinner",
            std::time::Duration::from_millis(900),
            |svg, delta| {
                svg.with_transformation(gpui::Transformation::rotate(gpui::radians(
                    delta * std::f32::consts::TAU,
                )))
            },
        )
        .into_any_element()
}
