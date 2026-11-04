//! How strong the chat's body text reads: an agent's reply and the user's own messages.

const STEP: f64 = 10.0;

/// The saved Text strength as the document carries it: 0 to 100 percent, snapped to its 10% steps,
/// `None` at the default, 0%.
///
/// CDXC:SessionChat 2026-10-31 DECISION:
/// User: "add a slider to pick the color of the text how strong it is (color of text in the chat now is not full white, i have user wanting it to be full white". The slider runs from today's colour (0%, the default, so nobody's look changes) up to full white in dark mode and full black in light mode. It stops at today's colour on the low end: going dimmer would close the gap to the muted text below it, which is the hierarchy this keeps. It moves only the body text and the headings inside it; inline code inside a sentence follows it; muted text, links, fenced code blocks and status colours stay where they are. The value rides the chat settings and the chat read, so the desktop, the web build and the phone draw the same strength.
/// SEE-ALSO: apps/desktop/src/app/native_chat/appearance.rs (`strengthened`, `reply_lane`), apps/mobile/app/src/chat/native/transcript/theme.ts (`strengthenedText`), server/src/session_chat_read.rs (`chatTextStrength` on every read).
pub fn text_strength(percent: f64) -> Option<u8> {
    if !percent.is_finite() {
        return None;
    }
    let snapped = ((percent.clamp(0.0, 100.0) / STEP).round() * STEP) as u8;
    (snapped != 0).then_some(snapped)
}

/// A body text colour's red, green and blue (0 to 1) at `percent`: unchanged at 0, full white in
/// dark mode or full black in light mode at 100.
pub fn strengthened_channels(base: [f32; 3], percent: u8, light: bool) -> [f32; 3] {
    if percent == 0 {
        return base;
    }
    if percent >= 100 {
        return if light { [0.0; 3] } else { [1.0; 3] };
    }
    let ink = if light { 0.0 } else { 1.0 };
    let amount = f32::from(percent) / 100.0;
    base.map(|channel| channel + (ink - channel) * amount)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DARK_BODY: [f32; 3] = [
        0xb4 as f32 / 255.0,
        0xb8 as f32 / 255.0,
        0xc0 as f32 / 255.0,
    ];
    const LIGHT_BODY: [f32; 3] = [
        0x4d as f32 / 255.0,
        0x4d as f32 / 255.0,
        0x50 as f32 / 255.0,
    ];

    #[test]
    fn the_default_is_no_strength_at_all() {
        assert_eq!(text_strength(0.0), None);
        assert_eq!(strengthened_channels(DARK_BODY, 0, false), DARK_BODY);
        assert_eq!(strengthened_channels(LIGHT_BODY, 0, true), LIGHT_BODY);
    }

    #[test]
    fn full_is_exactly_white_in_dark_mode_and_black_in_light_mode() {
        assert_eq!(text_strength(100.0), Some(100));
        assert_eq!(
            strengthened_channels(DARK_BODY, 100, false),
            [1.0, 1.0, 1.0]
        );
        assert_eq!(
            strengthened_channels(LIGHT_BODY, 100, true),
            [0.0, 0.0, 0.0]
        );
    }

    #[test]
    fn values_clamp_and_snap_to_the_slider_steps() {
        assert_eq!(text_strength(-20.0), None);
        assert_eq!(text_strength(240.0), Some(100));
        assert_eq!(text_strength(44.0), Some(40));
        assert_eq!(text_strength(46.0), Some(50));
        assert_eq!(text_strength(f64::NAN), None);
    }

    #[test]
    fn each_step_moves_toward_the_ink() {
        let mut last = DARK_BODY[0];
        for percent in (10..=100).step_by(10) {
            let [red, _, _] = strengthened_channels(DARK_BODY, percent, false);
            assert!(red > last, "{percent}");
            last = red;
        }
        let mut last = LIGHT_BODY[0];
        for percent in (10..=100).step_by(10) {
            let [red, _, _] = strengthened_channels(LIGHT_BODY, percent, true);
            assert!(red < last, "{percent}");
            last = red;
        }
    }
}
