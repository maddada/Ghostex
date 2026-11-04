//! Settings > Chat: the Marked sections style dropdown with its Preview button, the Handover heading
//! color strength slider (CDXC:MarkedHandovers 2026-10-20 in
//! packages/gx-chat-core/src/transcript/marked_sections.rs), and the Preview
//! dialog that draws the same sample reply in all three styles side by side.
//!
//! CDXC:MarkedHandovers 2026-10-13 DECISION:
//! User: a dropdown under Marked handovers picks the style (coloured labels by default), with a Preview button that opens a modal showing all three styles side by side on the same sample message; both are hidden while Marked handovers is off (the chat keeps colouring orchestrators' and threads' marked handovers in the saved style). 2026-10-13, superseding the icon-only eye button and the style-first order: "please swap these 2 and move the eye icon to the left of the dropdown and name it "Preview"" (Show marker emojis comes first; the button is the eye icon with the text Preview, left of the dropdown).
//!
//! CDXC:MarkedHandovers 2026-10-11 WHY: the preview is a dialog nested in the Settings window, not an app modal of its own: opening another app modal replaces the open one (`open_native_app_modal`), which would close Settings under the reader. It draws with the chat's own look file, included here as a module of its own because the Settings page also builds into the standalone modal demo, which has no chat renderer.

#[allow(dead_code)]
#[path = "../../../../native_chat/marked_look.rs"]
mod marked_look;
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    ButtonVariant, RowSpec, SELECT_WIDTH, icon, reset_key, setting_row, settings_button,
    settings_dialog_sheet, settings_select, toggle_field,
};
use super::{GeneralCx, GeneralTab, save};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use marked_look::{MarkedLook, MarkerStyle, preview_reply};
use serde_json::json;

const KEY: &str = "markedHandoversStyle";
const EMOJI_KEY: &str = "markedHandoversEmoji";
const STRENGTH_KEY: &str = "markedHandoversHeadingStrength";

impl GeneralTab {
    /// The style row and the emoji switch under it, while Marked handovers is on.
    pub(super) fn marked_sections_style_row(
        &mut self,
        g: &GeneralCx,
        section: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if !g.values.bool("markedHandovers") {
            return Vec::new();
        }
        let mut rows: Vec<AnyElement> = Vec::new();
        if g.visible(section, EMOJI_KEY) {
            let spec = g
                .spec(
                    EMOJI_KEY,
                    "Show marker emojis",
                    "Show the marker emoji (❓ ✅ ⚠️ and the rest) at the start of each coloured section, in every style. Off: the colour and the label show the kind.",
                )
                .dependent();
            rows.push(toggle_field(
                self,
                &g.p,
                EMOJI_KEY,
                spec,
                g.values.bool(EMOJI_KEY),
                cx,
            ));
        }
        rows.extend(self.marked_style_select_row(g, section, window, cx));
        rows.extend(self.slider(
            g,
            section,
            STRENGTH_KEY,
            "Heading colour strength",
            "How strongly the coloured section headings in agents' handovers (Done, Needs you, Untested and the rest) stand out. 100% is full colour; 0% leaves them in plain bold text.",
            (
                g.number("MIN_MARKED_HANDOVERS_HEADING_STRENGTH_PERCENT"),
                g.number("MAX_MARKED_HANDOVERS_HEADING_STRENGTH_PERCENT"),
                g.number("MARKED_HANDOVERS_HEADING_STRENGTH_PERCENT_STEP"),
            ),
            true,
            window,
            cx,
        ));
        rows
    }

    fn marked_style_select_row(
        &mut self,
        g: &GeneralCx,
        section: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !g.visible(section, KEY) {
            return None;
        }
        let options = g.options("MARKED_HANDOVERS_STYLE_OPTIONS");
        let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
        let value = g.values.choice(KEY, &allowed);
        let spec: RowSpec = g
            .spec(
                KEY,
                "Marked sections style",
                "How the chat colours the sections an agent marks.",
            )
            .dependent();
        let select = settings_select(
            self,
            &g.p,
            KEY,
            &options,
            &value,
            Some(SELECT_WIDTH),
            false,
            None,
            move |page: &mut Self, next, _window, cx| save(page, KEY, json!(next), cx),
            window,
            cx,
        );
        let preview = settings_button(
            &g.p,
            "marked-sections-preview",
            "Preview",
            Some(icon::EYE),
            ButtonVariant::Outline,
            false,
            None,
            |page: &mut Self, window, cx| {
                let focus = cx.focus_handle();
                focus.focus(window, cx);
                page.marked_preview = Some(focus);
                cx.notify();
            },
            cx,
        );
        let control = h_flex()
            .gap(px(8.0))
            .child(preview)
            .child(select)
            .into_any_element();
        Some(setting_row(
            &g.p,
            KEY,
            spec,
            Some(reset_key::<Self>(KEY)),
            control,
            cx,
        ))
    }

    /// The Preview dialog, while it is open.
    pub(super) fn marked_sections_preview(
        &mut self,
        g: &GeneralCx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let focus = self.marked_preview.clone()?;
        let strength = g.values.f64(STRENGTH_KEY).clamp(0.0, 100.0) / 100.0;
        let chat = MarkedLook::chat_default(g.p.light, g.values.bool(EMOJI_KEY), strength as f32);
        let chat_background = gpui::rgb(if chat.light { 0xffffff } else { 0x0d0d0d });
        let paragraph_gap = gpui::rems(chat.block_gap / 16.0);
        let prose = chat.prose;
        let text = move |id: String, content: String| -> AnyElement {
            let mut style = gpui_component::text::TextViewStyle::default()
                .default_cursor(true)
                .paragraph_gap(paragraph_gap);
            style.is_dark = !chat.light;
            gpui_component::text::TextView::markdown(id, content)
                .parse_synchronously(true)
                .min_w_0()
                .style(style)
                .text_size(px(14.0))
                .line_height(px(22.75))
                .text_color(prose)
                .into_any_element()
        };
        let selected = MarkerStyle::from_value(Some(g.values.string(KEY).as_str()));
        let column = |letter: &str, name: &str, style: MarkerStyle| {
            let chosen = style == selected;
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(8.0))
                .child(
                    h_flex()
                        .gap(px(8.0))
                        .child(
                            div()
                                .px(px(7.0))
                                .rounded(px(6.0))
                                .border_1()
                                .border_color(hsla(g.p.hairline))
                                .text_size(px(12.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(hsla(g.p.muted))
                                .child(letter.to_string()),
                        )
                        .child(
                            div()
                                .text_size(px(14.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(name.to_string()),
                        )
                        // The heading of the style in use says so.
                        .when(chosen, |row| {
                            row.child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(hsla(g.p.muted))
                                    .child("In use"),
                            )
                        }),
                )
                .child(
                    div()
                        .w_full()
                        .rounded(px(12.0))
                        .border_1()
                        .border_color(chat.border)
                        .bg(chat_background)
                        .p(px(16.0))
                        .child(preview_reply(style, &chat, text)),
                )
                .into_any_element()
        };
        let body = vec![
            h_flex()
                .w_full()
                .items_start()
                .gap(px(16.0))
                .child(column("A", "Thin left rule", MarkerStyle::Rule))
                .child(column("C", "Coloured badge", MarkerStyle::Badge))
                .child(column("D", "Coloured labels", MarkerStyle::Labels))
                .into_any_element(),
        ];
        let max_height = f32::from(window.viewport_size().height) * 0.9;
        Some(settings_dialog_sheet(
            &g.p,
            "marked-sections-preview",
            &focus,
            "Marked sections styles",
            Some(
                div()
                    .child("The same reply in each style.")
                    .into_any_element(),
            ),
            1180.0,
            Some(max_height),
            24.0,
            20.0,
            true,
            body,
            None,
            |page: &mut Self, _window, cx| {
                page.marked_preview = None;
                cx.notify();
            },
            window,
            cx,
        ))
    }
}
