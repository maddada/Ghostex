//! The Interface page's Interface scale section: one Scale for the sidebar, the chat, Files and
//! the terminal text, and Scale areas separately, which swaps it for a slider per area
//! (CDXC:Settings 2026-11-05 DECISION in the settings catalog's `interface_scale.rs`).
use super::super::super::catalog::{module, settings_catalog};
use super::super::super::fields::{
    SliderBinding, reset_key, settings_section, slider_number_field, toggle_field_with,
};
#[cfg(target_os = "linux")]
use super::super::super::{catalog::SettingOption, fields::select_field};
use super::{ThemeCx, ThemeTab};
use ghostex_settings_catalog::interface_scale::{
    INTERFACE_SCALE_SETTING, MAX_INTERFACE_SCALE_PERCENT, MIN_INTERFACE_SCALE_PERCENT,
    SCALE_AREAS_SEPARATELY_SETTING, SCALED_AREA_SETTINGS, SIDEBAR_ZOOM_RANGE,
    TERMINAL_FONT_SIZE_RANGE, area_value_at_scale, interface_scale_percent, scale_areas_separately,
};
use gpui::{AnyElement, Context, IntoElement, Window};
use serde_json::{Map, json};

const SIDEBAR_ZOOM_KEY: &str = "agentManagerZoomPercent";
const CHAT_ZOOM_KEY: &str = "sessionChatZoomPercent";
const FILES_ZOOM_KEY: &str = crate::ui_fonts::FILES_ZOOM_SETTING;
const TERMINAL_FONT_SIZE_KEY: &str = "terminalFontSize";

impl ThemeTab {
    pub(super) fn interface_scale_section(
        &mut self,
        t: &ThemeCx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // A search hit on any of the six rows shows the section as it stands, so a search for
        // "chat zoom" with the areas together lands on Scale and the switch, not on a slider that
        // would do nothing.
        if t.searching
            && !SCALED_AREA_SETTINGS
                .iter()
                .chain(
                    [
                        INTERFACE_SCALE_SETTING,
                        SCALE_AREAS_SEPARATELY_SETTING,
                        ghostex_settings_catalog::interface_scale::LINUX_DISPLAY_SCALE_SETTING,
                    ]
                    .iter(),
                )
                .any(|key| t.visible(key))
        {
            return None;
        }
        let p = t.p;
        let number = |name: &str| settings_catalog().number(module::SETTINGS, name);
        let chat_zoom_step = number("SESSION_CHAT_ZOOM_PERCENT_STEP");
        let separately = scale_areas_separately(t.values.map());
        let mut rows = Vec::new();
        if !separately {
            rows.push(slider_number_field(
                self,
                &p,
                t.spec(
                    INTERFACE_SCALE_SETTING,
                    "Scale (%)",
                    "The size of the sidebar, chats, files and terminal text, from 70% to 200%. The header, tabs, menus and dialogs keep their size. Zoom In and Zoom Out change it from anywhere, and Reset Zoom sets 100%.",
                ),
                Some(reset_key::<Self>(INTERFACE_SCALE_SETTING)),
                SliderBinding {
                    key: INTERFACE_SCALE_SETTING,
                    min: MIN_INTERFACE_SCALE_PERCENT,
                    max: MAX_INTERFACE_SCALE_PERCENT,
                    step: chat_zoom_step,
                },
                interface_scale_percent(t.values.map()),
                window,
                cx,
            ));
        }
        rows.push(toggle_field_with(
            &p,
            SCALE_AREAS_SEPARATELY_SETTING,
            t.spec(
                SCALE_AREAS_SEPARATELY_SETTING,
                "Scale areas separately",
                "Give the sidebar, chat, Files and terminal text their own sizes. Zoom In and Zoom Out then change only the area you are in.",
            ),
            separately,
            Some(reset_key::<Self>(SCALE_AREAS_SEPARATELY_SETTING)),
            |page: &mut Self, on, _window, cx| {
                let mut patch = Map::new();
                patch.insert(SCALE_AREAS_SEPARATELY_SETTING.to_string(), json!(on));
                if on {
                    // Each area starts at the size Scale gave it, so nothing jumps when the
                    // switch turns on.
                    let percent = interface_scale_percent(page.store.read(cx).values().map());
                    for key in SCALED_AREA_SETTINGS {
                        patch.insert(key.to_string(), json!(area_value_at_scale(key, percent)));
                    }
                }
                page.save_patch(patch, cx);
            },
            cx,
        ));
        #[cfg(target_os = "linux")]
        rows.push(display_scale_field(self, t, window, cx));
        if separately {
            let areas = [
                (
                    SIDEBAR_ZOOM_KEY,
                    "Sidebar zoom (%)",
                    "The size of the sidebar.",
                    SIDEBAR_ZOOM_RANGE.0,
                    SIDEBAR_ZOOM_RANGE.1,
                    chat_zoom_step,
                ),
                (
                    CHAT_ZOOM_KEY,
                    "Chat zoom (%)",
                    "The size of every chat, including messages and the prompt composer. Zoom In and Zoom Out in a chat change it too. The preview above draws at this size.",
                    number("MIN_SESSION_CHAT_ZOOM_PERCENT"),
                    number("MAX_SESSION_CHAT_ZOOM_PERCENT"),
                    chat_zoom_step,
                ),
                (
                    FILES_ZOOM_KEY,
                    "Files zoom (%)",
                    "The size of the text of a file open in the Files view, with its line numbers, and the Changes diff. Zoom In and Zoom Out in an open file change it too, and Reset Zoom sets 100%.",
                    number("MIN_SESSION_CHAT_ZOOM_PERCENT"),
                    number("MAX_SESSION_CHAT_ZOOM_PERCENT"),
                    chat_zoom_step,
                ),
                (
                    TERMINAL_FONT_SIZE_KEY,
                    "Terminal font size",
                    "The terminal text size in points.",
                    TERMINAL_FONT_SIZE_RANGE.0,
                    TERMINAL_FONT_SIZE_RANGE.1,
                    0.5,
                ),
            ];
            for (key, label, description, min, max, step) in areas {
                rows.push(slider_number_field(
                    self,
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
        }
        settings_section(
            &p,
            "Interface scale",
            Some("How big the sidebar, chats, files and terminal text are.".into()),
            None,
            rows,
        )
        .map(IntoElement::into_any_element)
    }
}

/// Settings > Interface > Display scale (Linux only): what Ghostex found at launch, and Automatic
/// or a fixed scale, which `linux_x11_scale::pin_gpui_x11_scale_factor` reads at the next launch
/// (CDXC:PlatformSupport 2026-11-08 DECISION in the settings catalog's `interface_scale.rs`).
#[cfg(target_os = "linux")]
fn display_scale_field(
    page: &mut ThemeTab,
    t: &ThemeCx,
    window: &mut Window,
    cx: &mut Context<ThemeTab>,
) -> AnyElement {
    use crate::linux_x11_scale::{ScaleSource, detected_scale};
    use ghostex_settings_catalog::interface_scale::{
        LINUX_DISPLAY_SCALE_OPTIONS, LINUX_DISPLAY_SCALE_SETTING,
    };

    let percent = |scale: f32| format!("{}%", (scale * 100.0).round() as i64);
    // X11 gives the whole app one scale, so the Settings window's is the app's.
    let running = window.scale_factor();
    let detected = detected_scale();
    let automatic = detected
        .and_then(|found| found.automatic)
        .unwrap_or(running);
    let found = match detected.map(|found| found.source) {
        Some(ScaleSource::XSettings) => "from your desktop's display settings",
        Some(ScaleSource::XResources) => "from your desktop's Xft.dpi",
        Some(ScaleSource::Hyprland) => "from Hyprland",
        Some(ScaleSource::Default) => "because your desktop didn't announce a scale",
        Some(ScaleSource::Environment) | Some(ScaleSource::Gpui) | None => "from your X11 session",
    };
    let mut description = if detected.is_some_and(|found| found.source == ScaleSource::Environment)
    {
        format!(
            "Ghostex is drawn at {}, set by GPUI_X11_SCALE_FACTOR, which wins over this setting.",
            percent(running)
        )
    } else {
        format!(
            "Ghostex is drawn at {}. Automatic uses {} {found}.",
            percent(running),
            percent(automatic)
        )
    };
    description.push_str(
        " This sizes the whole app, including the header, menus, dialogs and web pages; Interface scale applies on top. Takes effect when Ghostex restarts.",
    );
    let options: Vec<SettingOption> = LINUX_DISPLAY_SCALE_OPTIONS
        .iter()
        .map(|option| SettingOption {
            label: if option.value == "auto" {
                format!("Automatic ({})", percent(automatic))
            } else {
                option.label.to_string()
            },
            value: option.value.to_string(),
        })
        .collect();
    let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
    select_field(
        page,
        &t.p,
        LINUX_DISPLAY_SCALE_SETTING,
        t.spec(LINUX_DISPLAY_SCALE_SETTING, "Display scale", &description),
        Some(reset_key::<ThemeTab>(LINUX_DISPLAY_SCALE_SETTING)),
        &options,
        &t.values.choice(LINUX_DISPLAY_SCALE_SETTING, &allowed),
        None,
        |page: &mut ThemeTab, next, _window, cx| {
            page.save(LINUX_DISPLAY_SCALE_SETTING, json!(next), cx)
        },
        window,
        cx,
    )
}
