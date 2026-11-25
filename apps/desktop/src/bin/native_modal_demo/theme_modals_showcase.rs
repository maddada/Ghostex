//! Multi-theme showcase (`GHOSTEX_NATIVE_MODAL_DEMO=theme-modals`) rendering all 4 key modal
//! surfaces (Switch to Native? confirm dialog, Rename Session, Settings, and Account Switch Card)
//! through Ghostex's shared `ModalPalette`, `SettingsPalette`, and `AccountSwitchCardLook` across
//! every built-in theme (Default dark/light, all 12 gallery looks, all 20 named presets) plus
//! glass-off and bright-wallpaper readability checks.

use super::account_switch_card_view::{
    AccountSwitchCardHooks, AccountSwitchCardLook, render_account_switch_overlay,
};
use super::app::helpers::{apply_demo_theme_roles, demo_modal_surface_rgba};
use super::native_modal_kit::{
    MODAL_RADIUS_CONTROL, MODAL_RADIUS_SECTION, ModalPalette, SwitchOff, SwitchShape, css_mix,
    draw_switch, hsla, neutral_button_fills, rgba_of,
};
use super::settings_modal::palette::SettingsPalette;
use super::{DemoEnv, open_modal_window};
use ghostex_settings_catalog::theme_colours::{
    sidebar_titlebar_pack_rgb, sidebar_titlebar_rgb_channels, theme_accent_for_variant,
    work_area_background_for_variant,
};
use ghostex_settings_catalog::theme_roles::roles_for;
use ghostex_settings_catalog::{Platform, artwork_by_id, artwork_file_name, theme_gallery};
use gpui::{
    AnyElement, App, AppContext as _, BoxShadow, Context, FontWeight, Hsla, IntoElement, ObjectFit,
    ParentElement as _, Render, Rgba, Styled as _, StyledImage as _, Window, div, img, point, px,
    rgb, svg,
};
use serde_json::{Map, Value, json};
use std::path::PathBuf;

/// Mac frosted glass alphas matching `helpers/window_glass.rs`.
const GLASS_ALPHA_DARK: f32 = 0.68;
const GLASS_ALPHA_LIGHT: f32 = 0.78;

#[derive(Clone)]
struct ThemeBundle {
    name: String,
    light: bool,
    glass: bool,
    bright_backdrop: bool,
    work_area: Hsla,
    modal: ModalPalette,
    settings: SettingsPalette,
    account: AccountSwitchCardLook,
    art_path: Option<PathBuf>,
}

fn find_art_path(id: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let artwork = artwork_by_id(id)?;
    let path = PathBuf::from(home)
        .join(".local/share/ghostex/components/theme-art")
        .join(artwork_file_name(artwork));
    path.is_file().then_some(path)
}

fn settings_map_for_theme(
    theme_id: &str,
    light: bool,
) -> (String, Map<String, Value>, Option<PathBuf>) {
    let gallery = theme_gallery(Platform::current());
    let mut settings = Map::new();
    let mut name = if light {
        "Default (Light)".to_string()
    } else {
        "Default (Dark)".to_string()
    };
    let mut art_id = if light {
        "gm-light".to_string()
    } else {
        "polar-night".to_string()
    };

    if theme_id != "default" {
        let gallery = gallery.to_value();
        if let Some(entry) = gallery
            .as_array()
            .into_iter()
            .flatten()
            .find(|item| item.get("id").and_then(Value::as_str) == Some(theme_id))
        {
            if let Some(label) = entry.get("name").and_then(Value::as_str) {
                name = label.to_string();
            }
            let mut dark_roles = Map::new();
            let mut light_roles = Map::new();
            if let Some(changes) = entry.get("changes").and_then(Value::as_object) {
                for (key, value) in changes {
                    let Some(text) = value.as_str() else {
                        continue;
                    };
                    if let Some(role) = key.strip_prefix("dark.") {
                        if text != "default" {
                            dark_roles.insert(role.to_string(), json!(text));
                        }
                        continue;
                    }
                    if let Some(role) = key.strip_prefix("light.") {
                        if text != "default" {
                            light_roles.insert(role.to_string(), json!(text));
                        }
                        continue;
                    }
                    if key == "windowArt" && !light {
                        art_id = text.to_string();
                    }
                    if key == "windowArtLight" && light {
                        art_id = text.to_string();
                    }
                    if matches!(
                        key.as_str(),
                        "themeSidebarContrast"
                            | "themeWorkAreaContrast"
                            | "customSidebarTitlebarBackgroundDarknessPercent"
                            | "customSidebarTitlebarLightBackgroundLightnessPercent"
                            | "windowArtDim"
                            | "windowArtBlur"
                            | "windowGlassSidebarOpacityDark"
                            | "windowGlassWorkAreaTintDark"
                    ) {
                        if let Ok(num) = text.parse::<f64>() {
                            settings.insert(key.clone(), json!(num));
                            continue;
                        }
                    }
                    settings.insert(key.clone(), json!(text));
                }
            }
            // Also check preset's own artwork id when present locally.
            if find_art_path(theme_id).is_some() {
                art_id = theme_id.to_string();
            }
            let mut roles_obj = Map::new();
            if !dark_roles.is_empty() {
                roles_obj.insert("dark".to_string(), Value::Object(dark_roles));
            }
            if !light_roles.is_empty() {
                roles_obj.insert("light".to_string(), Value::Object(light_roles));
            }
            // Presets explicitly clear roles not in their list; populate an empty object for
            // the active appearance so catalog default roles don't bleed into named presets.
            let active_key = if light { "light" } else { "dark" };
            roles_obj
                .entry(active_key.to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            settings.insert("themeRoles".to_string(), Value::Object(roles_obj));
        }
    }
    let art_path = find_art_path(&art_id);
    (name, settings, art_path)
}

fn build_account_look(
    settings: &Map<String, Value>,
    light: bool,
    glass: bool,
) -> AccountSwitchCardLook {
    let background_rgb = work_area_background_for_variant(settings, light);
    let background: Hsla = rgb(background_rgb).into();
    let toward = |target: f32, amount: f32| -> Hsla {
        let [r, g, b] = sidebar_titlebar_rgb_channels(background_rgb);
        rgb(sidebar_titlebar_pack_rgb([
            r + (target - r) * amount,
            g + (target - g) * amount,
            b + (target - b) * amount,
        ]))
        .into()
    };
    let lifted = |dark_amount: f32, light_amount: f32| -> Hsla {
        if light {
            toward(0.0, light_amount)
        } else {
            toward(255.0, dark_amount)
        }
    };
    let raised = |amount: f32| -> Hsla { toward(255.0, amount) };

    let mut card_panel: Hsla = if glass {
        if light {
            gpui::white().opacity(0.5)
        } else {
            gpui::white().opacity(0.06)
        }
    } else if light {
        raised(0.6)
    } else {
        lifted(0.035, 0.0)
    };
    let mut composer_background: Hsla = if glass {
        if light {
            gpui::black().opacity(0.04)
        } else {
            gpui::white().opacity(0.06)
        }
    } else if light {
        raised(0.3)
    } else {
        lifted(0.03, 0.0)
    };
    let mut border: Hsla = if glass {
        if light {
            gpui::black().opacity(0.10)
        } else {
            gpui::white().opacity(0.09)
        }
    } else {
        lifted(0.062, 0.095)
    };
    let mut composer_border: Hsla = border;
    let mut input_border: Hsla = if light {
        toward(0.0, 0.095)
    } else {
        gpui::white().opacity(0.09)
    };
    let mut foreground: Hsla = rgb(if light { 0x27272a } else { 0xfcfcfc }).into();
    let mut prose: Hsla = rgb(if light { 0x4d4d50 } else { 0xb4b8c0 }).into();
    let mut muted: Hsla = rgb(if light { 0x71717b } else { 0x9e9e9e }).into();
    let mut accent: Hsla = rgb(theme_accent_for_variant(settings, light)).into();

    let roles = roles_for(settings, light);
    let role_hsla = |id: &str| -> Option<Hsla> {
        let value = roles.iter().find(|(role, _)| *role == id)?.1;
        let [r, g, b, a] = value.rgba_on_base(light, background_rgb);
        Some(Hsla::from(Rgba { r, g, b, a }))
    };
    if let Some(c) = role_hsla("border") {
        border = c;
        composer_border = c;
        input_border = c;
    }
    if let Some(c) = role_hsla("text") {
        foreground = c;
        prose = c;
    }
    if let Some(c) = role_hsla("mutedText") {
        muted = c;
    }
    if let Some(c) = role_hsla("surface") {
        card_panel = c;
    }
    if let Some(c) = role_hsla("chatBox") {
        composer_background = c;
    }
    if let Some(c) = role_hsla("accent") {
        accent = background.blend(c);
    }

    let fills = neutral_button_fills(light);
    AccountSwitchCardLook {
        background,
        card_panel,
        composer_background,
        composer_border,
        border,
        input_border,
        foreground,
        prose,
        muted,
        accent,
        brand_color: rgb(0xd97757).into(),
        button_rest: fills.rest.into(),
        button_hover: fills.hover.into(),
        button_pressed: fills.pressed.into(),
        light,
        window_glass: glass,
        scale: 1.0,
        font: ".SystemUIFont".to_string(),
    }
}

fn build_theme_bundle(
    theme_id: &str,
    light: bool,
    glass: bool,
    bright_backdrop: bool,
    label_override: Option<&str>,
) -> ThemeBundle {
    let (mut name, settings, art_path) = settings_map_for_theme(theme_id, light);
    if let Some(custom) = label_override {
        name = custom.to_string();
    }
    apply_demo_theme_roles(&settings);
    let surface = demo_modal_surface_rgba(light);
    let solid_modal = ModalPalette::resolve(light, None)
        .tinted(surface)
        .with_theme_roles();
    let modal = if glass {
        let alpha = if light {
            GLASS_ALPHA_LIGHT
        } else {
            GLASS_ALPHA_DARK
        };
        solid_modal.frosted_dialog(rgba_of(solid_modal.surface, alpha))
    } else {
        solid_modal
    };
    let settings_palette = SettingsPalette::resolve(&modal);
    let account = build_account_look(&settings, light, glass);
    let work_area: Hsla = rgb(work_area_background_for_variant(&settings, light)).into();
    ThemeBundle {
        name,
        light,
        glass,
        bright_backdrop,
        work_area,
        modal,
        settings: settings_palette,
        account,
        art_path,
    }
}

fn modal_card_shadows(light: bool) -> Vec<BoxShadow> {
    let alpha = if light { 0.16 } else { 0.45 };
    vec![
        BoxShadow {
            color: gpui::black().opacity(alpha),
            offset: point(px(0.0), px(16.0)),
            blur_radius: px(38.0),
            spread_radius: px(-8.0),
            inset: false,
        },
        BoxShadow {
            color: gpui::black().opacity(alpha * 0.45),
            offset: point(px(0.0), px(4.0)),
            blur_radius: px(12.0),
            spread_radius: px(-4.0),
            inset: false,
        },
    ]
}

fn sample_account_card_json() -> Value {
    json!({
        "id": "demo-switching",
        "provider": "claude",
        "phase": "switching",
        "verified": false,
        "dismissible": false,
        "heading": "Switching Claude account",
        "lede": "Your conversation will be ready for your next message.",
        "from": {
            "label": "y•••a@•••••.••• – SP",
            "target": false,
            "usage": [
                { "label": "5h limit", "used": 2.0, "level": "low", "reset": "3h 6m" },
                { "label": "7d limit", "used": 3.0, "level": "low", "reset": "4d 1h" },
                { "label": "Fable", "used": 0.0, "level": "low", "reset": "4d 1h" }
            ]
        },
        "to": {
            "label": "x•••0@•••••.••• – 70",
            "target": true,
            "usage": [
                { "label": "5h limit", "used": null, "level": "unknown", "reset": null },
                { "label": "7d limit", "used": 4.0, "level": "low", "reset": "6d 13h" },
                { "label": "Fable", "used": 0.0, "level": "low", "reset": "6d 13h" }
            ]
        },
        "steps": [
            { "label": "Switch account", "state": "active" },
            { "label": "Resume conversation", "state": "pending" }
        ]
    })
}

/// Renders the backdrop behind modals for one theme tile or quadrant: real artwork (dimmed +
/// work-area tinted like Ghostex), or bright sunlit sky/snow gradient for bright-wallpaper checks,
/// or solid work-area surface when glass is off.
fn backdrop_layer(bundle: &ThemeBundle) -> AnyElement {
    let mut layer = div()
        .absolute()
        .inset_0()
        .overflow_hidden()
        .bg(bundle.work_area);
    if bundle.bright_backdrop {
        // Bright daylight sky + snow/cloud glow to stress-test readability over bright wallpaper.
        layer = layer
            .bg(gpui::linear_gradient(
                145.0,
                gpui::linear_color_stop(rgb(0xe8f3ff), 0.0),
                gpui::linear_color_stop(rgb(0xfff6e5), 1.0),
            ))
            .child(
                div()
                    .absolute()
                    .top(px(-60.0))
                    .left(px(80.0))
                    .size(px(340.0))
                    .rounded_full()
                    .bg(rgb(0xffffff).opacity(0.75)),
            )
            .child(
                div()
                    .absolute()
                    .bottom(px(-80.0))
                    .right(px(60.0))
                    .size(px(380.0))
                    .rounded_full()
                    .bg(rgb(0xa8d8ff).opacity(0.65)),
            );
    } else if bundle.glass {
        if let Some(path) = bundle.art_path.clone() {
            let dim_wash = if bundle.light {
                gpui::white().opacity(0.32)
            } else {
                gpui::black().opacity(0.35)
            };
            layer = layer
                .child(
                    img(path)
                        .absolute()
                        .inset_0()
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                )
                .child(div().absolute().inset_0().bg(dim_wash))
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(bundle.work_area.opacity(0.42)),
                );
        } else {
            // Subtle themed atmospheric glow for presets without cached artwork.
            layer = layer
                .child(
                    div()
                        .absolute()
                        .top(px(-40.0))
                        .left(px(-30.0))
                        .w(px(300.0))
                        .h(px(220.0))
                        .rounded_full()
                        .bg(hsla(bundle.modal.primary).opacity(if bundle.light {
                            0.28
                        } else {
                            0.24
                        })),
                )
                .child(
                    div()
                        .absolute()
                        .bottom(px(-50.0))
                        .right(px(-20.0))
                        .w(px(320.0))
                        .h(px(240.0))
                        .rounded_full()
                        .bg(hsla(bundle.modal.success).opacity(if bundle.light {
                            0.22
                        } else {
                            0.18
                        })),
                );
        }
    }
    layer.into_any_element()
}

/// Renders a faithful full-size Switch to Native? confirm dialog using `ModalPalette`.
fn render_confirm_modal(p: &ModalPalette, compact: bool) -> AnyElement {
    let neutral = neutral_button_fills(p.light);
    let primary = p.primary_button_fills();
    let pad = if compact { 11.0 } else { 22.0 };
    let gap = if compact { 8.0 } else { 16.0 };
    let title_sz = if compact { 11.5 } else { 16.0 };
    let body_sz = if compact { 9.5 } else { 13.0 };
    let btn_h = if compact { 22.0 } else { 32.0 };
    let btn_sz = if compact { 10.0 } else { 13.5 };
    div()
        .w_full()
        .max_w(px(if compact { 250.0 } else { 460.0 }))
        .rounded(px(MODAL_RADIUS_SECTION))
        .border_1()
        .border_color(hsla(p.window_border()))
        .bg(hsla(p.surface))
        .shadow(modal_card_shadows(p.light))
        .p(px(pad))
        .flex()
        .flex_col()
        .gap(px(gap))
        .text_color(hsla(p.foreground))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(if compact { 3.0 } else { 6.0 }))
                .child(
                    div()
                        .text_size(px(title_sz))
                        .font_weight(FontWeight::MEDIUM)
                        .child("Switch to Native?"),
                )
                .child(
                    div()
                        .text_size(px(body_sz))
                        .line_height(px(body_sz * 1.45))
                        .text_color(hsla(p.muted))
                        .child(if compact {
                            "Moves the chat into Ghostex’s built-in view."
                        } else {
                            "This moves the conversation into Ghostex’s built-in chat view and restarts Claude with the conversation intact."
                        }),
                ),
        )
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(if compact { 6.0 } else { 10.0 }))
                .px(px(if compact { 8.0 } else { 12.0 }))
                .py(px(if compact { 5.0 } else { 9.0 }))
                .rounded(px(MODAL_RADIUS_CONTROL))
                .border_1()
                .border_color(hsla(p.hairline))
                .bg(hsla(p.panel))
                .child(
                    div()
                        .size(px(if compact { 12.0 } else { 16.0 }))
                        .rounded(px(4.0))
                        .bg(hsla(p.primary))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            svg()
                                .path("titlebar/check.svg")
                                .size(px(if compact { 9.0 } else { 12.0 }))
                                .text_color(hsla(p.primary_foreground)),
                        ),
                )
                .child(
                    div()
                        .text_size(px(body_sz))
                        .text_color(hsla(p.foreground))
                        .child("Use Native for new chats with Claude"),
                ),
        )
        .child(
            div()
                .w_full()
                .flex()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .h(px(btn_h))
                        .rounded(px(MODAL_RADIUS_CONTROL))
                        .bg(hsla(neutral.rest))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(btn_sz))
                        .text_color(hsla(p.foreground))
                        .child("Cancel"),
                )
                .child(
                    div()
                        .flex_1()
                        .h(px(btn_h))
                        .rounded(px(MODAL_RADIUS_CONTROL))
                        .bg(hsla(primary.rest))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(btn_sz))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(hsla(p.primary_button_text()))
                        .child("Switch to Native"),
                ),
        )
        .into_any_element()
}

/// Renders a faithful Rename Session dialog using `ModalPalette`.
fn render_rename_modal(p: &ModalPalette, compact: bool) -> AnyElement {
    let neutral = neutral_button_fills(p.light);
    let primary = p.primary_button_fills();
    let pad = if compact { 11.0 } else { 22.0 };
    let gap = if compact { 8.0 } else { 16.0 };
    let title_sz = if compact { 11.5 } else { 16.0 };
    let body_sz = if compact { 9.5 } else { 13.0 };
    let btn_h = if compact { 22.0 } else { 32.0 };
    let btn_sz = if compact { 10.0 } else { 13.5 };
    div()
        .w_full()
        .max_w(px(if compact { 250.0 } else { 510.0 }))
        .rounded(px(MODAL_RADIUS_SECTION))
        .border_1()
        .border_color(hsla(p.window_border()))
        .bg(hsla(p.surface))
        .shadow(modal_card_shadows(p.light))
        .p(px(pad))
        .flex()
        .flex_col()
        .gap(px(gap))
        .text_color(hsla(p.foreground))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(if compact { 3.0 } else { 6.0 }))
                .child(
                    div()
                        .text_size(px(title_sz))
                        .font_weight(FontWeight::MEDIUM)
                        .child("Rename Session"),
                )
                .child(
                    div()
                        .text_size(px(body_sz))
                        .line_height(px(body_sz * 1.45))
                        .text_color(hsla(p.muted))
                        .child(if compact {
                            "Type a name, or paste longer text to generate one."
                        } else {
                            "Type a name, or paste longer text to generate one. Leave it empty to name it from this session."
                        }),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(if compact { 4.0 } else { 6.0 }))
                .child(
                    div()
                        .w_full()
                        .px(px(if compact { 8.0 } else { 12.0 }))
                        .py(px(if compact { 6.0 } else { 10.0 }))
                        .rounded(px(MODAL_RADIUS_CONTROL))
                        .border_1()
                        .border_color(hsla(p.focus_border))
                        .bg(hsla(p.raised))
                        .text_size(px(if compact { 10.0 } else { 14.0 }))
                        .text_color(hsla(p.foreground))
                        .child("Theme glass for all modals"),
                )
                .child(
                    div()
                        .text_size(px(if compact { 9.0 } else { 12.5 }))
                        .text_color(hsla(p.muted))
                        .child("26 / 70"),
                ),
        )
        .child(
            div()
                .w_full()
                .flex()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .h(px(btn_h))
                        .rounded(px(MODAL_RADIUS_CONTROL))
                        .bg(hsla(neutral.rest))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(btn_sz))
                        .text_color(hsla(p.foreground))
                        .child("Cancel"),
                )
                .child(
                    div()
                        .flex_1()
                        .h(px(btn_h))
                        .rounded(px(MODAL_RADIUS_CONTROL))
                        .bg(hsla(primary.rest))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(btn_sz))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(hsla(p.primary_button_text()))
                        .child("Rename"),
                ),
        )
        .into_any_element()
}

/// Renders a faithful Settings modal preview using `SettingsPalette`.
fn render_settings_modal(s: &SettingsPalette, compact: bool) -> AnyElement {
    let neutral = neutral_button_fills(s.light);
    let primary = s.modal.primary_button_fills();
    let pad = if compact { 8.0 } else { 16.0 };
    let rail_w = if compact { 76.0 } else { 154.0 };
    let item_sz = if compact { 9.0 } else { 12.5 };
    let row_sz = if compact { 9.5 } else { 13.0 };
    let sub_sz = if compact { 8.5 } else { 11.5 };
    let rail_item = |label: &'static str, active: bool| {
        div()
            .w_full()
            .px(px(if compact { 6.0 } else { 10.0 }))
            .py(px(if compact { 3.5 } else { 6.0 }))
            .rounded(px(6.0))
            .bg(if active {
                hsla(s.raised_hover)
            } else {
                gpui::transparent_black()
            })
            .text_size(px(item_sz))
            .font_weight(if active {
                FontWeight::MEDIUM
            } else {
                FontWeight::NORMAL
            })
            .text_color(hsla(if active { s.foreground } else { s.muted }))
            .child(label)
    };
    let sw = |checked: bool| {
        draw_switch(
            if compact {
                SwitchShape::SMALL
            } else {
                SwitchShape::REGULAR
            },
            SwitchOff {
                track: s.raised_hover,
                edge: Some(s.hairline),
                thumb: if s.light {
                    s.modal.solid_surface
                } else {
                    s.muted
                },
            },
            s.light,
            checked,
            false,
        )
    };
    div()
        .w_full()
        .max_w(px(if compact { 250.0 } else { 540.0 }))
        .rounded(px(MODAL_RADIUS_SECTION))
        .border_1()
        .border_color(hsla(s.modal.window_border()))
        .bg(hsla(s.surface))
        .shadow(modal_card_shadows(s.light))
        .p(px(pad))
        .flex()
        .gap(px(if compact { 8.0 } else { 14.0 }))
        .text_color(hsla(s.foreground))
        .child(
            // Settings left rail
            div()
                .w(px(rail_w))
                .flex_shrink_0()
                .rounded(px(8.0))
                .border_1()
                .border_color(hsla(s.hairline))
                .bg(hsla(s.raised))
                .p(px(if compact { 4.0 } else { 6.0 }))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(rail_item("General", false))
                .child(rail_item("Interface", true))
                .child(rail_item("Agents", false))
                .child(rail_item("Hotkeys", false))
                .child(rail_item("Remote", false)),
        )
        .child(
            // Settings page content
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(if compact { 6.0 } else { 10.0 }))
                .child(
                    // Search bar
                    div()
                        .w_full()
                        .px(px(if compact { 7.0 } else { 10.0 }))
                        .py(px(if compact { 4.0 } else { 6.0 }))
                        .rounded(px(MODAL_RADIUS_CONTROL))
                        .border_1()
                        .border_color(hsla(s.hairline))
                        .bg(hsla(s.raised))
                        .text_size(px(sub_sz))
                        .text_color(hsla(s.muted))
                        .child("Search settings..."),
                )
                .child(
                    // Card with rows
                    div()
                        .w_full()
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(hsla(s.hairline))
                        .bg(hsla(s.raised))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .px(px(if compact { 7.0 } else { 11.0 }))
                                .py(px(if compact { 5.0 } else { 8.0 }))
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .child(
                                            div()
                                                .text_size(px(row_sz))
                                                .font_weight(FontWeight::MEDIUM)
                                                .child("Transparency"),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(sub_sz))
                                                .text_color(hsla(s.muted))
                                                .child("Frosted glass panels"),
                                        ),
                                )
                                .child(sw(s.glass)),
                        )
                        .child(
                            div()
                                .px(px(if compact { 7.0 } else { 11.0 }))
                                .py(px(if compact { 5.0 } else { 8.0 }))
                                .border_t_1()
                                .border_color(hsla(s.hairline))
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .text_size(px(row_sz))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child("Accent"),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .child(
                                            div()
                                                .size(px(if compact { 10.0 } else { 14.0 }))
                                                .rounded_full()
                                                .bg(hsla(s.primary)),
                                        )
                                        .child(
                                            div()
                                                .px(px(if compact { 6.0 } else { 9.0 }))
                                                .py(px(if compact { 2.0 } else { 4.0 }))
                                                .rounded(px(6.0))
                                                .bg(hsla(neutral.rest))
                                                .text_size(px(sub_sz))
                                                .child("Reset"),
                                        )
                                        .child(
                                            div()
                                                .px(px(if compact { 7.0 } else { 10.0 }))
                                                .py(px(if compact { 2.0 } else { 4.0 }))
                                                .rounded(px(6.0))
                                                .bg(hsla(primary.rest))
                                                .text_size(px(sub_sz))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(hsla(s.modal.primary_button_text()))
                                                .child("Apply"),
                                        ),
                                ),
                        ),
                ),
        )
        .into_any_element()
}

/// Renders a compact Account Switch Card tile for dense multi-theme grids (using the exact same
/// palette math as `account_switch_card_view.rs`, without colliding fixed a11y node IDs when
/// rendering 14 cards in one GPUI window).
fn render_compact_account_card(look: &AccountSwitchCardLook) -> AnyElement {
    let base = look.background.alpha(1.0);
    let surface = if look.light {
        let solid = base.blend(look.card_panel);
        if look.window_glass {
            solid.alpha(0.86)
        } else {
            solid.alpha(1.0)
        }
    } else if look.window_glass {
        if look.composer_background.a >= 0.72 && look.composer_background.a <= 0.88 {
            look.composer_background
        } else {
            base.blend(look.composer_background).alpha(0.82)
        }
    } else {
        base.blend(look.composer_background).alpha(1.0)
    };
    let tile = if look.light {
        gpui::black().opacity(0.035)
    } else {
        gpui::white().opacity(0.045)
    };
    let line = if look.light && !look.window_glass {
        look.input_border
    } else {
        look.composer_border
    };
    let track = if look.light {
        gpui::black().opacity(0.08)
    } else {
        gpui::white().opacity(0.09)
    };
    let accent_ink: Hsla = super::native_modal_kit::contrast_ink_on(look.accent.into()).into();
    let usage = |label: &'static str, pct: &'static str, dimmed: bool| {
        div()
            .flex_1()
            .min_w_0()
            .px(px(6.0))
            .py(px(5.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(line)
            .bg(tile)
            .flex()
            .items_center()
            .justify_between()
            .gap(px(3.0))
            .child(
                div()
                    .text_size(px(8.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(look.foreground)
                    .child(label),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if dimmed { look.muted } else { look.foreground })
                    .child(pct),
            )
    };
    let row =
        |role: &'static str, acct: &'static str, pcts: [(&'static str, &'static str, bool); 3]| {
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.0))
                        .child(
                            div()
                                .w(px(24.0))
                                .text_size(px(8.5))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(look.muted)
                                .child(role),
                        )
                        .child(
                            div()
                                .text_size(px(9.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(look.foreground)
                                .child(acct),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(4.0))
                        .child(usage(pcts[0].0, pcts[0].1, pcts[0].2))
                        .child(usage(pcts[1].0, pcts[1].1, pcts[1].2))
                        .child(usage(pcts[2].0, pcts[2].1, pcts[2].2)),
                )
        };
    div()
        .w_full()
        .rounded(px(12.0))
        .border_1()
        .border_color(line)
        .bg(surface)
        .shadow(modal_card_shadows(look.light))
        .p(px(10.0))
        .flex()
        .flex_col()
        .gap(px(7.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(5.0))
                .child(
                    svg()
                        .path("agent-icons/claude.svg")
                        .size(px(11.0))
                        .text_color(look.brand_color),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(look.foreground)
                        .child("Switching Claude account"),
                ),
        )
        .child(row(
            "From",
            "y•••a – SP",
            [
                ("5h", "2%", false),
                ("7d", "3%", false),
                ("Fable", "0%", false),
            ],
        ))
        .child(row(
            "To",
            "x•••0 – 70",
            [
                ("5h", "–", true),
                ("7d", "4%", false),
                ("Fable", "0%", false),
            ],
        ))
        .child(
            div()
                .flex()
                .gap(px(6.0))
                .pt(px(2.0))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .size(px(12.0))
                                        .rounded_full()
                                        .bg(look.accent)
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_size(px(8.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(accent_ink)
                                        .child("1"),
                                )
                                .child(
                                    div()
                                        .text_size(px(8.5))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(look.foreground)
                                        .child("Switch account"),
                                ),
                        )
                        .child(
                            div().w_full().h(px(2.0)).rounded(px(2.0)).bg(track).child(
                                div()
                                    .w(gpui::relative(0.45))
                                    .h_full()
                                    .rounded(px(2.0))
                                    .bg(look.accent),
                            ),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .size(px(12.0))
                                        .rounded_full()
                                        .border_1()
                                        .border_color(line)
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_size(px(8.0))
                                        .text_color(look.muted)
                                        .child("2"),
                                )
                                .child(
                                    div()
                                        .text_size(px(8.5))
                                        .text_color(look.muted)
                                        .child("Resume"),
                                ),
                        )
                        .child(div().w_full().h(px(2.0)).rounded(px(2.0)).bg(track)),
                ),
        )
        .into_any_element()
}

/// One full-size 4-up showcase (2x2 quadrants for Switch to Native?, Rename, Settings, Account Switch).
fn render_quad_showcase(bundle: &ThemeBundle, header: &str) -> AnyElement {
    let card_json = sample_account_card_json();
    let label_bg = if bundle.light {
        rgba_of(rgb(0x000000), 0.08)
    } else {
        rgba_of(rgb(0xffffff), 0.10)
    };
    let tag = |text: &'static str| {
        div()
            .px(px(9.0))
            .py(px(3.0))
            .rounded(px(6.0))
            .bg(hsla(label_bg))
            .text_size(px(11.5))
            .font_weight(FontWeight::MEDIUM)
            .text_color(hsla(bundle.modal.foreground))
            .child(text)
    };
    div()
        .relative()
        .size_full()
        .font_family(".SystemUIFont")
        .child(backdrop_layer(bundle))
        .child(
            div()
                .relative()
                .size_full()
                .p(px(28.0))
                .flex()
                .flex_col()
                .gap(px(20.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(17.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(hsla(bundle.modal.foreground))
                                .child(header.to_string()),
                        )
                        .child(
                            div()
                                .px(px(10.0))
                                .py(px(4.0))
                                .rounded(px(8.0))
                                .border_1()
                                .border_color(hsla(bundle.modal.hairline))
                                .bg(hsla(bundle.modal.surface))
                                .text_size(px(12.0))
                                .text_color(hsla(bundle.modal.muted))
                                .child(if bundle.bright_backdrop {
                                    "Glass ON · Bright wallpaper"
                                } else if bundle.glass {
                                    "Glass ON · Translucent"
                                } else {
                                    "Glass OFF · Opaque theme surface"
                                }),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .gap(px(24.0))
                        .child(
                            // Left column: Confirm + Settings
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(20.0))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .items_center()
                                        .gap(px(10.0))
                                        .child(tag("Confirm dialog · Switch to Native?"))
                                        .child(render_confirm_modal(&bundle.modal, false)),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .flex()
                                        .flex_col()
                                        .items_center()
                                        .justify_center()
                                        .gap(px(10.0))
                                        .child(tag("Settings modal"))
                                        .child(render_settings_modal(&bundle.settings, false)),
                                ),
                        )
                        .child(
                            // Right column: Rename + Account Switch Card
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(20.0))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .items_center()
                                        .gap(px(10.0))
                                        .child(tag("Rename Session modal"))
                                        .child(render_rename_modal(&bundle.modal, false)),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .relative()
                                        .flex()
                                        .flex_col()
                                        .items_center()
                                        .justify_center()
                                        .gap(px(10.0))
                                        .child(tag("Account switch card"))
                                        .child(
                                            div()
                                                .relative()
                                                .w(px(520.0))
                                                .h(px(360.0))
                                                .rounded(px(16.0))
                                                .overflow_hidden()
                                                .child(render_account_switch_overlay(
                                                    &card_json,
                                                    520.0,
                                                    &bundle.account,
                                                    AccountSwitchCardHooks::default(),
                                                )),
                                        ),
                                ),
                        ),
                ),
        )
        .into_any_element()
}

/// One cell in a multi-theme grid showing all 4 modal surfaces for one built-in theme.
fn render_theme_grid_cell(bundle: &ThemeBundle) -> AnyElement {
    let badge_bg = css_mix(
        rgb(if bundle.light { 0xffffff } else { 0x000000 }),
        0.55,
        bundle.modal.solid_surface,
    );
    div()
        .relative()
        .flex_1()
        .min_w_0()
        .rounded(px(14.0))
        .overflow_hidden()
        .border_1()
        .border_color(hsla(bundle.modal.window_border()))
        .child(backdrop_layer(bundle))
        .child(
            div()
                .relative()
                .p(px(10.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .px(px(8.0))
                                .py(px(2.5))
                                .rounded(px(6.0))
                                .bg(hsla(badge_bg))
                                .border_1()
                                .border_color(hsla(bundle.modal.hairline))
                                .text_size(px(11.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(hsla(bundle.modal.foreground))
                                .child(bundle.name.clone()),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .size(px(10.0))
                                        .rounded_full()
                                        .bg(hsla(bundle.modal.solid_surface))
                                        .border_1()
                                        .border_color(hsla(bundle.modal.hairline)),
                                )
                                .child(
                                    div()
                                        .size(px(10.0))
                                        .rounded_full()
                                        .bg(hsla(bundle.modal.primary)),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(render_confirm_modal(&bundle.modal, true)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(render_rename_modal(&bundle.modal, true)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(render_settings_modal(&bundle.settings, true)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(render_compact_account_card(&bundle.account)),
                        ),
                ),
        )
        .into_any_element()
}

fn render_theme_grid(
    title: &str,
    bundles: &[ThemeBundle],
    cols: usize,
    light_canvas: bool,
) -> AnyElement {
    let canvas_bg = if light_canvas {
        rgb(0xe7e8ec)
    } else {
        rgb(0x0b0d11)
    };
    let title_color = if light_canvas {
        rgb(0x1f2328)
    } else {
        rgb(0xe6e8ec)
    };
    let mut rows = div().flex().flex_col().gap(px(14.0)).w_full();
    for chunk in bundles.chunks(cols) {
        let mut row = div().flex().gap(px(14.0)).w_full();
        for bundle in chunk {
            row = row.child(render_theme_grid_cell(bundle));
        }
        for _ in chunk.len()..cols {
            row = row.child(div().flex_1().min_w_0());
        }
        rows = rows.child(row);
    }
    div()
        .size_full()
        .bg(canvas_bg)
        .font_family(".SystemUIFont")
        .p(px(18.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .text_size(px(16.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(title_color)
                .child(title.to_string()),
        )
        .child(rows)
        .into_any_element()
}

struct ThemeModalsShowcase {
    mode: String,
    bundles: Vec<ThemeBundle>,
}

impl Render for ThemeModalsShowcase {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        match self.mode.as_str() {
            "default-dark" => render_quad_showcase(
                &self.bundles[0],
                "Default Theme (Polar Night Dark) · Glass ON · All Modals",
            ),
            "default-light" => render_quad_showcase(
                &self.bundles[0],
                "Default Theme (Everforest Light) · Glass ON · All Modals",
            ),
            "modes-compare" => render_theme_grid(
                "Glass OFF (Reduced Transparency) & Bright Wallpaper Readability · Default Theme",
                &self.bundles,
                2,
                false,
            ),
            "grid-gallery" => render_theme_grid(
                "All 12 Gallery / Art Built-in Themes · Switch to Native?, Rename, Settings, Account Card",
                &self.bundles,
                4,
                false,
            ),
            "grid-presets-dark" => render_theme_grid(
                "Default Dark + All 13 Dark Named Presets · Switch to Native?, Rename, Settings, Account Card",
                &self.bundles,
                4,
                false,
            ),
            "grid-presets-light" => render_theme_grid(
                "Default Light + All 7 Light Named Presets · Switch to Native?, Rename, Settings, Account Card",
                &self.bundles,
                4,
                true,
            ),
            _ => render_quad_showcase(
                &self.bundles[0],
                "Default Theme (Polar Night Dark) · Glass ON · All Modals",
            ),
        }
    }
}

pub(crate) fn open(demo: &DemoEnv, cx: &mut App) {
    let mode = if demo.state.is_empty() {
        "default-dark".to_string()
    } else {
        demo.state.clone()
    };
    let (width, height, bundles) = match mode.as_str() {
        "default-light" => (
            1800.0,
            1050.0,
            vec![build_theme_bundle("default", true, true, false, None)],
        ),
        "modes-compare" => (
            1850.0,
            1060.0,
            vec![
                build_theme_bundle(
                    "default",
                    false,
                    false,
                    false,
                    Some("Default Dark · Glass OFF (Opaque)"),
                ),
                build_theme_bundle(
                    "default",
                    true,
                    false,
                    false,
                    Some("Default Light · Glass OFF (Opaque)"),
                ),
                build_theme_bundle(
                    "default",
                    false,
                    true,
                    true,
                    Some("Default Dark · Glass ON over Bright Wallpaper"),
                ),
                build_theme_bundle(
                    "default",
                    true,
                    true,
                    true,
                    Some("Default Light · Glass ON over Bright Wallpaper"),
                ),
            ],
        ),
        "grid-gallery" => {
            let ids = [
                "moonwater",
                "polar-night",
                "glasshouse",
                "red-earth",
                "deep-current",
                "night-garden",
                "nocturne-canyon",
                "firefly-forest",
                "ember-forge",
                "synthwave-84",
                "inkstone",
                "orchid-dusk",
            ];
            let bundles = ids
                .into_iter()
                .map(|id| build_theme_bundle(id, false, true, false, None))
                .collect();
            (2320.0, 1460.0, bundles)
        }
        "grid-presets-dark" => {
            let ids = [
                "default",
                "mocha",
                "macchiato",
                "frappe",
                "dracula",
                "sol-dark",
                "tn-night",
                "tn-storm",
                "tn-moon",
                "nord",
                "kg-wave",
                "kg-dragon",
                "ef-dark",
                "gm-dark",
            ];
            let bundles = ids
                .into_iter()
                .map(|id| build_theme_bundle(id, false, true, false, None))
                .collect();
            (2320.0, 1900.0, bundles)
        }
        "grid-presets-light" => {
            let ids = [
                "default",
                "latte",
                "alucard",
                "sol-light",
                "tn-day",
                "kg-lotus",
                "ef-light",
                "gm-light",
            ];
            let bundles = ids
                .into_iter()
                .map(|id| build_theme_bundle(id, true, true, false, None))
                .collect();
            (2320.0, 1020.0, bundles)
        }
        _ => (
            1800.0,
            1050.0,
            vec![build_theme_bundle("default", false, true, false, None)],
        ),
    };
    let _ = open_modal_window(
        width,
        height,
        move |_, cx| cx.new(|_| ThemeModalsShowcase { mode, bundles }),
        cx,
    );
}
