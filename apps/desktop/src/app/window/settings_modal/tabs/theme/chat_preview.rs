//! The preview in the Interface page's Text section: a few lines of chat at the fonts picked and
//! the Text strength set.
//!
//! CDXC:SessionChat 2026-10-31 DECISION:
//! User: "move all these to the interface page in settings and put the font next to them so we can preview the font there" and "make the preview show a few lines of chat like user message and reply by agent and thinking by agent". Its colours, sizes and fonts come from `native_chat/chat_text_look.rs`, the file the transcript itself draws from, so it cannot drift from the chat. It shows a font waiting for Apply through an exact family, which skips the substitution the rest of the app still draws with.
#[allow(dead_code)]
#[path = "../../../../native_chat/chat_text_look.rs"]
mod chat_text_look;

use super::super::super::super::native_modal_kit::hsla;
use super::super::super::palette::SettingsPalette;
use super::super::super::store::SettingsValues;
use super::colours::{Rgb, preview_colours};
use crate::ui_fonts::{exact_family, monospace_default_face, monospace_replacement, picked_family};
use chat_text_look::{
    CHAT_BODY_FONT, CHAT_MONO_FONT, ChatTextColors, INPUT_STEP_DARK, INPUT_STEP_LIGHT,
    PROSE_LINE_HEIGHT, PROSE_SIZE, THINKING_GAP, USER_BUBBLE_PADDING, USER_BUBBLE_RADIUS,
    inline_code_fill, inline_code_font_scale, inline_code_padding_x, inline_code_radius,
    zoom_scale,
};
use gpui::{
    AnyElement, App, FontWeight, Hsla, IntoElement as _, ParentElement as _, SharedString,
    Styled as _, div, px, relative,
};
use gpui_component::{h_flex, v_flex};

/// Registers the chat's body font, which otherwise waits for the first chat to open: Settings can
/// open before one has, and the preview would draw in the system's fallback instead.
pub(super) fn register_preview_fonts(cx: &App) {
    static REGISTERED: std::sync::Once = std::sync::Once::new();
    REGISTERED.call_once(|| {
        let faces = crate::assets::DM_SANS_UPRIGHT_FACES
            .iter()
            .map(|face| std::borrow::Cow::Borrowed(*face))
            .collect();
        if let Err(error) = cx.text_system().add_fonts(faces) {
            eprintln!("Could not register the chat preview's fonts: {error}");
        }
    });
}

/// The family the preview's prose draws in: the pick waiting for Apply, else what the app draws.
pub(super) fn preview_body_family(pending: Option<&str>) -> SharedString {
    match pending {
        None => CHAT_BODY_FONT.into(),
        Some(pick) => exact_family(picked_family(pick).unwrap_or(CHAT_BODY_FONT)),
    }
}

/// The family the preview's code draws in: the pick waiting for Apply, else what the app draws.
pub(super) fn preview_mono_family(pending: Option<&str>) -> SharedString {
    let installed = crate::ui_fonts::installed_families();
    match pending {
        None => CHAT_MONO_FONT.into(),
        Some(pick) => {
            monospace_replacement(pick, installed.as_ref().map(|families| families.as_slice()))
                .map_or_else(monospace_default_face, |family| exact_family(&family))
        }
    }
}

fn hsla_of(color: Rgb) -> Hsla {
    gpui::Rgba {
        r: color.0[0] as f32 / 255.0,
        g: color.0[1] as f32 / 255.0,
        b: color.0[2] as f32 / 255.0,
        a: 1.0,
    }
    .into()
}

/// The user bubble's fill: the chat background a step toward white, or black in light mode.
fn bubble_fill(background: Rgb, light: bool) -> Hsla {
    let (target, amount) = if light {
        (0.0, f64::from(INPUT_STEP_LIGHT))
    } else {
        (255.0, f64::from(INPUT_STEP_DARK))
    };
    hsla_of(Rgb(background
        .0
        .map(|channel| channel + (target - channel) * amount)))
}

/// Whether the chat draws in its light look: its own Chat theme, else the app's.
fn chat_is_light(p: &SettingsPalette, values: &SettingsValues) -> bool {
    match values.string("sessionChatTheme").as_str() {
        "light" => true,
        "dark" => false,
        _ => p.light,
    }
}

pub(super) fn chat_preview(
    p: &SettingsPalette,
    values: &SettingsValues,
    body_family: SharedString,
    mono_family: SharedString,
) -> AnyElement {
    let light = chat_is_light(p, values);
    let colors = ChatTextColors::for_variant(light);
    let strength = ghostex_gx_chat_core::transcript::text_strength::text_strength(
        values.f64("sessionChatTextStrengthPercent"),
    )
    .unwrap_or(0);
    let muted_thinking = values.bool("sessionChatMutedThinking");
    // The chat's size under Interface scale, as every chat draws it.
    let scale = zoom_scale(
        ghostex_settings_catalog::interface_scale::scaled_area_value(
            values.map(),
            "sessionChatZoomPercent",
        )
        .unwrap_or(100.0) as f32,
    );
    let (_, background, _) = preview_colours(values, !light);
    let reply = colors.reply_prose(muted_thinking, strength);
    let line = move |color: Hsla| {
        div()
            .min_w_0()
            .text_size(px(PROSE_SIZE * scale))
            .line_height(px(PROSE_LINE_HEIGHT * scale))
            .text_color(color)
    };
    let user = h_flex().w_full().justify_end().child(
        div()
            .max_w(relative(0.8))
            .rounded(px(USER_BUBBLE_RADIUS * scale))
            .p(px(USER_BUBBLE_PADDING * scale))
            .bg(bubble_fill(background, light))
            .child(
                line(colors.user_prose(strength))
                    .child("Can you give this function a clearer name?"),
            ),
    );
    let thinking = line(colors.thinking(muted_thinking))
        .child("Checking where it's called before renaming it.");
    let code = div()
        .flex_shrink_0()
        .px(px(inline_code_padding_x() * scale))
        .rounded(px(inline_code_radius() * scale))
        .bg(inline_code_fill(light))
        .font_family(mono_family)
        .font_weight(FontWeight::NORMAL)
        .text_size(px(PROSE_SIZE * inline_code_font_scale() * scale))
        // Text strength leaves code at the reply's own colour.
        .text_color(colors.reply_base(muted_thinking))
        .child("greet_user(name)");
    let answer = h_flex()
        .flex_wrap()
        .items_center()
        .gap(px(4.0 * scale))
        .child(line(reply).child("Done. It's now"))
        .child(code)
        .child(line(reply).child("and both callers use it."));
    v_flex()
        .mt(px(8.0))
        .p(px(14.0))
        .gap(px(THINKING_GAP * scale))
        .rounded(px(8.0))
        .border_1()
        .border_color(hsla(p.hairline))
        .bg(hsla_of(background))
        .font_family(body_family)
        .child(user)
        .child(thinking)
        .child(answer)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_waiting_draws_what_the_app_draws() {
        assert_eq!(
            preview_body_family(None),
            SharedString::from(CHAT_BODY_FONT)
        );
        assert_eq!(
            preview_mono_family(None),
            SharedString::from(CHAT_MONO_FONT)
        );
    }

    #[test]
    fn a_waiting_font_draws_exactly_that_font() {
        assert_eq!(
            preview_body_family(Some("Georgia")),
            exact_family("Georgia")
        );
        assert_eq!(preview_mono_family(Some("Menlo")), exact_family("Menlo"));
    }

    #[test]
    fn a_waiting_default_draws_the_default_even_with_a_font_applied() {
        assert_eq!(preview_body_family(Some("")), exact_family(CHAT_BODY_FONT));
        assert_eq!(
            preview_mono_family(Some("JetBrains Mono")),
            monospace_default_face()
        );
    }
}
