//! The Interface page's Text section: the app and monospace fonts, picked from the fonts installed
//! on this computer, how strong the chat's text reads, and a preview of all of them. The chat and
//! Files zooms moved to the Interface scale section (`interface_scale.rs`).
use super::super::super::catalog::{module, settings_catalog};
use super::super::super::fields::{
    DropdownState, FontList, SliderBinding, font_picker_field, reset_key, settings_section,
    slider_number_field, toggle_field,
};
use super::super::general::terminal_font::normalize_ghostty_font_family;
use super::chat_preview::{chat_preview, preview_body_family, preview_mono_family};
use super::controls::ICON_FONT;
use super::{ThemeCx, ThemeTab};
use crate::ui_fonts::{APP_FONT_SETTING, MONOSPACE_FONT_SETTING};
use gpui::{AnyElement, Context, IntoElement, Window};
use serde_json::json;

const TEXT_STRENGTH_KEY: &str = "sessionChatTextStrengthPercent";
const MUTED_THINKING_KEY: &str = "sessionChatMutedThinking";
const ICON_TEXT_STRENGTH: &str = "modals/settings/eye.svg";
const ICON_MUTED_THINKING: &str = "modals/settings/sparkles.svg";

fn font_dropdown(page: &mut ThemeTab) -> &mut DropdownState {
    &mut page.font_dropdown
}

fn pending_font(page: &mut ThemeTab) -> &mut Option<String> {
    &mut page.pending_font
}

fn monospace_font_dropdown(page: &mut ThemeTab) -> &mut DropdownState {
    &mut page.monospace_font_dropdown
}

fn pending_monospace_font(page: &mut ThemeTab) -> &mut Option<String> {
    &mut page.pending_monospace_font
}

impl ThemeTab {
    pub(super) fn font_section(
        &mut self,
        t: &ThemeCx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let p = t.p;
        let number = |name: &str| settings_catalog().number(module::SETTINGS, name);
        let mut rows = Vec::new();
        if t.visible(APP_FONT_SETTING) {
            let spec = t
                .spec(
                    APP_FONT_SETTING,
                    "App font",
                    "Used across the whole app: sidebar, chat, menus, dialogs and Settings. The preview shows your pick first; Apply redraws every window with it. Agents and sessions keep running.",
                )
                .icon(ICON_FONT);
            let value = t.values.string(APP_FONT_SETTING);
            rows.push(font_picker_field(
                self,
                &p,
                APP_FONT_SETTING,
                FontList::App,
                spec,
                Some(reset_key::<Self>(APP_FONT_SETTING)),
                &value,
                font_dropdown,
                pending_font,
                |page: &mut Self, next, _window, cx| page.save(APP_FONT_SETTING, json!(next), cx),
                window,
                cx,
            ));
        }
        if t.visible(MONOSPACE_FONT_SETTING) {
            let spec = t
                .spec(
                    MONOSPACE_FONT_SETTING,
                    "Monospace font",
                    "Used for terminals, code in the Files view, diffs, code blocks and tool output in chat. Pick an installed font, or type a Ghostty font-family name in the list's search field. Apply redraws everything with it; terminals keep running.",
                )
                .icon(ICON_FONT);
            // The value React showed was `normalizeghostexSettings(settings)`, so a legacy preset
            // name reads as the family it saves as.
            let value = normalize_ghostty_font_family(&t.values.string(MONOSPACE_FONT_SETTING));
            rows.push(font_picker_field(
                self,
                &p,
                MONOSPACE_FONT_SETTING,
                FontList::Monospace,
                spec,
                Some(reset_key::<Self>(MONOSPACE_FONT_SETTING)),
                &value,
                monospace_font_dropdown,
                pending_monospace_font,
                |page: &mut Self, next, _window, cx| {
                    page.save(
                        MONOSPACE_FONT_SETTING,
                        json!(normalize_ghostty_font_family(&next)),
                        cx,
                    )
                },
                window,
                cx,
            ));
        }
        if t.visible(TEXT_STRENGTH_KEY) {
            rows.push(slider_number_field(
                self,
                &p,
                t.spec(
                    TEXT_STRENGTH_KEY,
                    "Text strength",
                    "How strong agents' replies and your messages read in chat. 0% is the standard colour; 100% is full white in dark mode and full black in light mode. Thinking and code keep their colours.",
                )
                // Every row of the Text card has its icon, so the titles line up (CDXC:Settings
                // 2026-11-10 on `settings_note_row` in fields/row.rs).
                .icon(ICON_TEXT_STRENGTH),
                Some(reset_key::<Self>(TEXT_STRENGTH_KEY)),
                SliderBinding {
                    key: TEXT_STRENGTH_KEY,
                    min: number("MIN_SESSION_CHAT_TEXT_STRENGTH_PERCENT"),
                    max: number("MAX_SESSION_CHAT_TEXT_STRENGTH_PERCENT"),
                    step: number("SESSION_CHAT_TEXT_STRENGTH_PERCENT_STEP"),
                },
                t.values.f64(TEXT_STRENGTH_KEY),
                window,
                cx,
            ));
        }
        if t.visible(MUTED_THINKING_KEY) {
            rows.push(toggle_field(
                self,
                &p,
                MUTED_THINKING_KEY,
                t.spec(
                    MUTED_THINKING_KEY,
                    "Show thinking in grey",
                    "Show the agent's thinking in grey, so it reads apart from the agent's replies.",
                )
                .icon(ICON_MUTED_THINKING),
                t.values.bool(MUTED_THINKING_KEY),
                cx,
            ));
        }
        if rows.is_empty() {
            return None;
        }
        rows.push(chat_preview(
            &p,
            &t.values,
            preview_body_family(self.pending_font.as_deref()),
            preview_mono_family(self.pending_monospace_font.as_deref()),
        ));
        settings_section(
            &p,
            "Text",
            Some("The fonts, and how the chat's text reads.".into()),
            None,
            rows,
        )
        .map(IntoElement::into_any_element)
    }
}
