//! Text & Fonts' Terminal text: how the Monospace font draws in terminals (its weight, bold text,
//! line height and letter spacing). They were rows of the Terminal page until CDXC:Settings
//! 2026-11-19 in the settings catalog's general/theme.rs.
use super::super::super::fields::{
    SliderBinding, reset_key, settings_section, slider_number_field, toggle_field,
};
use super::{ThemeCx, ThemeTab};
use gpui::{AnyElement, Context, IntoElement, Window};

const BOLD_KEY: &str = "terminalBoldUsesHeavierFont";

impl ThemeTab {
    pub(super) fn terminal_text_section(
        &mut self,
        t: &ThemeCx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let p = t.p;
        let mut rows = Vec::new();
        let slider = |page: &mut Self,
                      rows: &mut Vec<AnyElement>,
                      key: &'static str,
                      label: &str,
                      description: &str,
                      (min, max, step): (f64, f64, f64),
                      window: &mut Window,
                      cx: &mut Context<Self>| {
            if t.visible(key) {
                rows.push(slider_number_field(
                    page,
                    &p,
                    t.spec(key, label, description),
                    Some(reset_key::<Self>(key)),
                    SliderBinding {
                        key,
                        min,
                        max,
                        step,
                    },
                    t.values.f64(key),
                    window,
                    cx,
                ));
            }
        };
        slider(
            self,
            &mut rows,
            "terminalFontWeight",
            "Font weight",
            "How heavy terminal text is drawn.",
            (100.0, 900.0, 50.0),
            window,
            cx,
        );
        if t.visible(BOLD_KEY) {
            rows.push(toggle_field(
                self,
                &p,
                BOLD_KEY,
                t.spec(
                    BOLD_KEY,
                    "Bold text uses a heavier font",
                    "Draw bold terminal text in a heavier weight. Turn off to keep bold text at your normal font weight, so it stands out by color only. A font-style-bold or font-variation-bold line in Ghostex's terminal config takes priority.",
                ),
                t.values.bool(BOLD_KEY),
                cx,
            ));
        }
        slider(
            self,
            &mut rows,
            "terminalLineHeight",
            "Line height",
            "The height of each terminal row.",
            (0.8, 2.0, 0.1),
            window,
            cx,
        );
        slider(
            self,
            &mut rows,
            "terminalLetterSpacing",
            "Letter spacing",
            "The space between terminal characters.",
            (-2.0, 8.0, 0.1),
            window,
            cx,
        );
        settings_section(
            &p,
            t.section_title("terminalText", "Terminal text"),
            None,
            None,
            rows,
        )
        .map(IntoElement::into_any_element)
    }
}
