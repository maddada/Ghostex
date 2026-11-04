use gpui::{Hsla, Window, rgb};
use serde_json::Value;

use crate::app::helpers::*;

/// The sidebar's text size, before the zoom.
pub(crate) const SIDEBAR_TEXT_SIZE: f32 = 15.55;

#[derive(Clone)]
pub(crate) struct SidebarAppearance {
    pub(crate) light: bool,
    /// Whether this sidebar sits on window glass: in the main window and in the floating reveal
    /// panel, never in other windows of its own.
    pub(crate) glass: bool,
    pub(crate) session_selected: Hsla,
    pub(crate) session_outline: Hsla,
    pub(crate) tooltip_delay: std::time::Duration,
    pub(crate) selected_outline: Hsla,
    pub(crate) selected_highlight: Hsla,
    pub(crate) foreground: Hsla,
    pub(crate) muted: Hsla,
    /// CDXC:Sidebar 2026-10-29 DECISION:
    /// User, comparing the sidebar's icons with the workarea header's (the sidebar's were #7D828C, the Spaces row's #808080 and the machine tab names #B5B8BF against the header's about #BBBBBC), picked "B. Sidebar takes the header's grey". The icons of the sidebar's top row, bottom bar, Spaces row and machine tabs use `titlebar_icon_color()`, the header's own icon colour, and the machine tab names use `titlebar_text_color()`, its label colour, so dark and light follow the header. Selected, hovered and disabled states keep their logic on top of this base. This supersedes the 2026-10-28 rule that drew Claude's button in #7C828C (app/claude_web/sidebar_button.rs).
    pub(crate) chrome_icon: Hsla,
    /// The header's label colour, for the machine tab names (see `chrome_icon`).
    pub(crate) chrome_label: Hsla,
    pub(crate) selected: Hsla,
    pub(crate) hover: Hsla,
    pub(crate) session_hover: Hsla,
    pub(crate) visible: Hsla,
    pub(crate) scale: f32,
}

impl SidebarAppearance {
    /// CDXC:Sidebar 2026-09-30 DECISION:
    /// User: the sidebar text and the chat's main text must be the same size. The sidebar scales only by its own zoom on top of GPUI's window scale, like the chat, and no longer shrinks to Chromium's display scale on Linux (that 2026-09-18 workaround made it smaller than the chat whenever GPUI's scale and Chromium's differed); Linux window scale is fixed once for every surface in `linux_x11_scale::pin_gpui_x11_scale_factor`.
    pub(crate) fn from_hud(hud: &Value, window: &Window) -> Self {
        let scale = hud
            .get("agentManagerZoomPercent")
            .and_then(Value::as_f64)
            .unwrap_or(100.0) as f32
            / 100.0;
        // CDXC:Theming 2026-10-17 WHY: The sidebar's own light flag was read from the Appearance setting and the system appearance on every draw, while its text and chrome colours (`chrome_color`, `titlebar_background`) follow the appearance the chrome was last themed with. When the two disagreed (always in Ghostex web, whose chrome stays dark while a light browser reads as a light system; on the computer between a system appearance change and the next settings refresh) the selected row got the light fill under dark-theme text. Every sidebar colour now follows the chrome's flag.
        let light = chrome_uses_light_appearance();
        let base = titlebar_background().blend(rgb(0).opacity(0.04).into());
        let glass = window_glass_active_in(window);
        let foreground = titlebar_active_text_color();
        /*
        CDXC:Sidebar 2026-09-23 DECISION:
        User: the active machine tab, Space and session "don't look good on light mode when glass is enabled", and the collapsed project holding the active session goes with them. On light-mode glass they share one translucent white wash and a soft outline, so the frosted tint shows through instead of an opaque white or grey slab; dark glass and the opaque sidebar keep their fills.
        */
        let light_glass = light && glass;
        let light_glass_selected: Hsla = rgb(0xffffff).opacity(0.55).into();
        Self {
            light,
            glass,
            session_selected: if light_glass {
                light_glass_selected
            } else if light {
                rgb(0xe7e7e7).into()
            } else {
                rgb(0xffffff).opacity(0.12).into()
            },
            session_outline: chrome_ink()
                .opacity(if light_glass {
                    0.10
                } else if light {
                    0.22
                } else {
                    0.08
                })
                .into(),
            tooltip_delay: tooltip_delay_from_hud(hud),
            foreground: chrome_color(0xb4b8c0, 0x262626).into(),
            muted: chrome_color(0x7c828c, 0x6b7280).into(),
            chrome_icon: titlebar_icon_color(),
            chrome_label: titlebar_text_color(),
            selected: if light_glass {
                light_glass_selected
            } else {
                rgb(0xffffff).opacity(if light { 1.0 } else { 0.12 }).into()
            },
            selected_outline: chrome_ink()
                .opacity(if light_glass {
                    0.10
                } else if light {
                    0.12
                } else {
                    0.08
                })
                .into(),
            selected_highlight: rgb(0xffffff).opacity(if light { 0.6 } else { 0.04 }).into(),
            hover: rgb(0x808080).opacity(0.12).into(),
            // Under window glass these are washes of the same ink rather than opaque blends of the
            // chrome colour, which would sit on the frosted sidebar as solid slabs.
            session_hover: if glass {
                foreground.opacity(if light { 0.12 } else { 0.16 })
            } else {
                base.blend(foreground.opacity(if light { 0.12 } else { 0.22 }))
            },
            visible: if glass {
                foreground.opacity(if light { 0.06 } else { 0.12 })
            } else if light {
                base.blend(foreground.opacity(0.06))
            } else {
                base.blend(foreground.opacity(0.30))
                    .blend(rgb(0).opacity(0.40).into())
            },
            scale,
        }
    }
}

fn tooltip_delay_from_hud(hud: &Value) -> std::time::Duration {
    std::time::Duration::from_millis(
        hud["settings"]["sidebarTooltipDelayMs"]
            .as_u64()
            .unwrap_or(500),
    )
}

impl crate::GhostexGpuiApp {
    /// The user's Tooltip Delay setting, for native chrome outside the sidebar
    /// that must wait as long as the sidebar's own tooltips do.
    pub(crate) fn configured_tooltip_delay(&self) -> std::time::Duration {
        match self.native_sidebar.snapshot.as_ref() {
            Some(snapshot) => tooltip_delay_from_hud(&snapshot.hud),
            None => tooltip_delay_from_hud(&Value::Null),
        }
    }
}
