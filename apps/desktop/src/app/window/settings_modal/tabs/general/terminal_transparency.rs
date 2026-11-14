//! Terminal's Transparency switch and, under it while it is on, the Background opacity row. The
//! opacity acts on the window's glass, so it works only while transparency is on; the rule and the user's decision are on `TERMINAL_GLASS_PERCENT` in
//! app/helpers/terminal_glass.rs.
use super::super::super::fields::{reset_key, toggle_field_with};
use super::{GeneralCx, GeneralTab, save};
use gpui::{AnyElement, Context, Window};
use serde_json::{Value, json};

/// Why the window shows nothing through its glass, when the system's own switch is the reason.
fn system_note(blocked: bool) -> &'static str {
    match (blocked, cfg!(target_os = "windows")) {
        (false, _) => "",
        (true, true) => {
            " Transparency effects is off in Windows Settings > Personalization > Colors, so the window stays opaque until you turn it on there and restart Ghostex."
        }
        (true, false) if cfg!(target_os = "linux") => {
            super::super::theme::transparency::LINUX_NO_BLUR_NOTE
        }
        (true, false) => {
            " Reduce transparency is on in System Settings > Accessibility > Display, so the window stays opaque until you turn it off."
        }
    }
}

pub(super) fn transparency_rows(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Vec<AnyElement> {
    let s = "terminal";
    let p = g.p;
    let glass = g.values.string("windowGlass");
    let glass_on = glass != "opaque";
    let blocked = page
        .store
        .read(cx)
        .hud()
        .and_then(|hud| hud.get("windowGlassBlockedBySystem"))
        .and_then(Value::as_bool)
        == Some(true);
    let note = system_note(blocked);
    let mut rows = Vec::new();
    // CDXC:Settings 2026-11-14 DECISION (AGENTS.md; Q3110, user: "fix all these too but make sure
    // ux makes sense after fix"; supersedes the 2026-10-14 part that showed this row disabled,
    // with a note and a Turn on transparency button, while transparency was off): Terminal shows
    // the window's Transparency as a switch, and Background opacity (%) under it as a ↳ row only
    // while it is on. Turning the switch on picks Dark only, as the button did; Settings >
    // Interface > Transparency keeps Always and the rest.
    if g.visible(s, "terminalBackgroundOpacity") {
        let spec = g.spec(
            "windowGlass",
            "Transparency",
            &format!(
                "Lets terminals show what is behind the window. On picks Dark only; Settings > Interface > Transparency has Always and the window's other glass settings.{note}"
            ),
        );
        rows.push(toggle_field_with(
            &p,
            "terminalWindowGlass",
            spec,
            glass_on,
            Some(reset_key::<GeneralTab>("windowGlass")),
            // `glass_for_transparency`: turning transparency on from Never picks Dark only.
            |page: &mut GeneralTab, on, _window, cx| {
                save(
                    page,
                    "windowGlass",
                    json!(if on { "auto" } else { "opaque" }),
                    cx,
                )
            },
            cx,
        ));
        if glass_on {
            let description = format!(
                "How much of the work area's glass tint terminals keep. 100% matches the rest of the work area; lower shows more of what is behind the window.{}",
                if glass == "auto" {
                    " Transparency is dark mode only."
                } else {
                    ""
                },
            );
            rows.extend(page.slider(
                g,
                s,
                "terminalBackgroundOpacity",
                "Background opacity (%)",
                &description,
                (0.0, 100.0, 1.0),
                true,
                window,
                cx,
            ));
        }
    }
    // CDXC:Settings 2026-10-29 DECISION (cleanup Q15): the Background blur row is gone from this page; the
    // window has one blur, set under Settings > Interface > Transparency.
    rows
}
