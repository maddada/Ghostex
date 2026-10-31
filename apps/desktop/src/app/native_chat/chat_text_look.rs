//! The colours, fonts and sizes of the chat's body text: your messages, an agent's thinking and its
//! reply, at the Text strength and Show thinking in grey settings.
//!
//! The file depends only on gpui, serde_json and the chat core, so Settings includes it as its own
//! module for the preview on the Interface page (`settings_modal/tabs/theme/chat_preview.rs`): the
//! Settings page also builds into the standalone modal demo, which has no chat renderer to call
//! into. The transcript draws from these same answers, so the preview cannot drift from it.
//!
//! SEE-ALSO: apps/desktop/src/app/native_chat/appearance.rs (`thinking_lane`, `reply_lane`),
//! apps/mobile/app/src/chat/native/transcript/theme.ts (`strengthenedText`).
use gpui::{Hsla, rgb};
use std::sync::LazyLock;

pub(crate) const CHAT_BODY_FONT: &str = "DM Sans";
pub(crate) const CHAT_MONO_FONT: &str = "JetBrainsMono Nerd Font";

pub(crate) const PROSE_SIZE: f32 = 14.0;
pub(crate) const PROSE_LINE_HEIGHT: f32 = 22.75;

/// The user's message bubble, before the zoom.
pub(crate) const USER_BUBBLE_RADIUS: f32 = 16.0;
pub(crate) const USER_BUBBLE_PADDING: f32 = 12.0;

/// How far the bubble's fill steps off the chat background: toward white in dark mode, toward black
/// in light mode.
pub(crate) const INPUT_STEP_DARK: f32 = 0.03;
pub(crate) const INPUT_STEP_LIGHT: f32 = 0.032;

/// Inline code's size as a share of the prose around it.
pub(crate) fn inline_code_font_scale() -> f32 {
    static SCALE: LazyLock<f32> = LazyLock::new(|| {
        let visual: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../packages/gx-chat-core/visual/markdown-visual.json"
        ))
        .expect("shared Markdown appearance");
        visual["inlineCode"]["fontScale"]
            .as_f64()
            .expect("inline code's font scale") as f32
    });
    *SCALE
}

/// Inline code's fill.
pub(crate) fn inline_code_fill(light: bool) -> Hsla {
    rgb(if light { 0xefeff0 } else { 0x272727 }).into()
}

/// The chat's text colours in one appearance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChatTextColors {
    pub(crate) foreground: Hsla,
    pub(crate) primary: Hsla,
    pub(crate) prose: Hsla,
    pub(crate) muted: Hsla,
    pub(crate) light: bool,
}

impl ChatTextColors {
    pub(crate) fn for_variant(light: bool) -> Self {
        let color =
            |dark: u32, light_color: u32| rgb(if light { light_color } else { dark }).into();
        Self {
            foreground: color(0xfcfcfc, 0x27272a),
            primary: color(0xb4b8c0, 0x27272a),
            prose: color(0xb4b8c0, 0x4d4d50),
            muted: color(0x9e9e9e, 0x71717b),
            light,
        }
    }

    /// A body text colour at Text strength `percent` (CDXC:SessionChat 2026-10-31 in
    /// packages/gx-chat-core/src/transcript/text_strength.rs).
    pub(crate) fn strengthened(&self, color: Hsla, percent: u8) -> Hsla {
        if percent == 0 {
            return color;
        }
        let base = gpui::Rgba::from(color);
        let [r, g, b] = ghostex_gx_chat_core::transcript::text_strength::strengthened_channels(
            [base.r, base.g, base.b],
            percent,
            self.light,
        );
        gpui::Rgba { r, g, b, a: base.a }.into()
    }

    /// A user message's text.
    pub(crate) fn user_prose(&self, percent: u8) -> Hsla {
        self.strengthened(self.primary, percent)
    }

    /// An agent's thinking: Text strength leaves it alone, so it stays apart from the reply. Light
    /// mode always mutes it; dark mode draws it like the answer unless Grey thinking is on.
    pub(crate) fn thinking(&self, muted_thinking: bool) -> Hsla {
        if muted_thinking || self.light {
            self.muted
        } else {
            self.primary
        }
    }

    /// An agent's reply before Text strength: the full foreground while Grey thinking is on, so the
    /// two read apart.
    pub(crate) fn reply_base(&self, muted_thinking: bool) -> Hsla {
        if muted_thinking {
            self.foreground
        } else {
            self.prose
        }
    }

    /// An agent's reply at Text strength `percent`.
    pub(crate) fn reply_prose(&self, muted_thinking: bool, percent: u8) -> Hsla {
        self.strengthened(self.reply_base(muted_thinking), percent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_strength_raises_the_reply_and_your_message_but_never_thinking() {
        for light in [false, true] {
            let colors = ChatTextColors::for_variant(light);
            for muted in [false, true] {
                let thinking = colors.thinking(muted);
                assert_ne!(colors.reply_prose(muted, 100), thinking, "{light} {muted}");
                // Thinking's colour takes no strength to compute at all.
                assert_eq!(colors.thinking(muted), thinking);
            }
            assert_ne!(colors.user_prose(100), colors.user_prose(0));
            assert_ne!(colors.reply_prose(false, 100), colors.reply_prose(false, 0));
        }
    }

    #[test]
    fn full_strength_is_full_white_in_dark_mode_and_full_black_in_light_mode() {
        let dark = gpui::Rgba::from(ChatTextColors::for_variant(false).reply_prose(false, 100));
        assert!(dark.r > 0.99 && dark.g > 0.99 && dark.b > 0.99);
        let light = gpui::Rgba::from(ChatTextColors::for_variant(true).reply_prose(false, 100));
        assert!(light.r < 0.01 && light.g < 0.01 && light.b < 0.01);
    }

    #[test]
    fn grey_thinking_mutes_thinking_and_lifts_the_reply() {
        let colors = ChatTextColors::for_variant(false);
        assert_eq!(colors.thinking(true), colors.muted);
        assert_eq!(colors.reply_base(true), colors.foreground);
        assert_eq!(colors.thinking(false), colors.primary);
        assert_eq!(colors.reply_base(false), colors.prose);
    }

    #[test]
    fn inline_code_has_a_size_from_the_shared_appearance() {
        assert!(inline_code_font_scale() > 0.0);
    }
}
