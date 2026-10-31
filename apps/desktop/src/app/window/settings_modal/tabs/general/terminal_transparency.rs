//! Terminal's Background opacity row. It acts on the window's glass, so it works only while Enable
//! transparency is on; the rule and the user's decision are on `TERMINAL_GLASS_PERCENT` in
//! app/helpers/terminal_glass.rs.
use super::super::super::fields::{ButtonVariant, reset_key, setting_row, settings_button};
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
    if g.visible(s, "terminalBackgroundOpacity") {
        if glass_on {
            let description = format!(
                "How much of the work area's glass tint terminals keep. 100% matches the rest of the work area; lower shows more of what is behind the window. Applies while transparency is on (Settings > Interface > Transparency){}.{note}",
                if glass == "auto" {
                    ", which is dark mode only"
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
                false,
                window,
                cx,
            ));
        } else {
            let restart = if cfg!(target_os = "windows") {
                " On Windows it takes effect the next time Ghostex starts."
            } else {
                ""
            };
            let spec = g
                .spec(
                    "terminalBackgroundOpacity",
                    "Background opacity (%)",
                    &format!(
                        "Terminals can only be see-through while the window is. Turn on transparency to show what is behind the window through them; it picks Dark only, which you can change under Settings > Interface > Transparency.{restart}"
                    ),
                )
                .readout("Needs transparency");
            let control = settings_button(
                &p,
                "terminal-turn-on-transparency",
                "Turn on transparency",
                None,
                ButtonVariant::Outline,
                false,
                None,
                // `glass_for_transparency`: turning transparency on from Never picks Dark only.
                |page: &mut GeneralTab, _window, cx| save(page, "windowGlass", json!("auto"), cx),
                cx,
            );
            rows.push(setting_row(
                &p,
                "terminalBackgroundOpacity",
                spec,
                Some(reset_key::<GeneralTab>("terminalBackgroundOpacity")),
                control,
                cx,
            ));
        }
    }
    // CDXC:Settings 2026-10-29 DECISION (cleanup Q15): the Background blur row is gone from this page; the
    // window has one blur, set under Settings > Interface > Transparency.
    rows
}
