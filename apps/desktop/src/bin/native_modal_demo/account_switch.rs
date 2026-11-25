//! Account-switch card preview (`GHOSTEX_NATIVE_MODAL_DEMO=account-switch`): draws the chat's
//! own `account_switch_card_view::render_account_switch_overlay` over a default-theme chat surface
//! (Polar Night Dark with its wallpaper and composer/pill strip, or Everforest Light) using
//! Ghostex's built-in default theme tokens from `ghostex_settings_catalog`.
//! States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): `switching` (default), `compare` (card over the
//! wallpaper directly above the composer without a dimming scrim over the composer, for side-by-side
//! comparison), `resuming`, `failed`.

use super::account_switch_card_view::{
    AccountSwitchCardHooks, AccountSwitchCardLook, render_account_switch_overlay,
};
use super::native_modal_kit::neutral_button_fills;
use super::{DemoEnv, open_modal_window};
use ghostex_settings_catalog::theme_colours::{
    sidebar_titlebar_pack_rgb, sidebar_titlebar_rgb_channels, theme_accent_for_variant,
    work_area_background_for_variant,
};
use ghostex_settings_catalog::theme_roles::roles_for;
use gpui::{
    AnyElement, App, AppContext as _, Context, FontWeight, Hsla, IntoElement, ObjectFit,
    ParentElement as _, Render, Rgba, Styled as _, StyledImage as _, Window, div, img, px, rgb,
    svg,
};
use serde_json::{Map, Value, json};
use std::path::PathBuf;

const PREVIEW_WIDTH: f32 = 1650.0;
const PREVIEW_HEIGHT: f32 = 962.0;

struct AccountSwitchPreview {
    state: String,
    light: bool,
    glass: bool,
    art_path: Option<PathBuf>,
}

fn find_polar_night_art() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let path = PathBuf::from(home)
        .join(".local/share/ghostex/components/theme-art/polar-night-a7184e97.webp");
    path.is_file().then_some(path)
}

fn default_chat_look(light: bool, glass: bool) -> AccountSwitchCardLook {
    let settings = Map::new();
    let background_rgb = work_area_background_for_variant(&settings, light);
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
            gpui::black().opacity(0.08)
        } else {
            gpui::white().opacity(0.08)
        }
    } else {
        lifted(0.062, 0.095)
    };
    let mut composer_border: Hsla = if glass {
        if light {
            gpui::black().opacity(0.08)
        } else {
            gpui::white().opacity(0.08)
        }
    } else {
        lifted(0.08, 0.067)
    };
    let mut input_border: Hsla = if light {
        toward(0.0, 0.095)
    } else {
        gpui::white().opacity(0.08)
    };
    let mut foreground: Hsla = rgb(if light { 0x27272a } else { 0xfcfcfc }).into();
    let mut prose: Hsla = rgb(if light { 0x4d4d50 } else { 0xb4b8c0 }).into();
    let mut muted: Hsla = rgb(if light { 0x71717b } else { 0x9e9e9e }).into();
    let mut accent: Hsla = rgb(theme_accent_for_variant(&settings, light)).into();

    let roles = roles_for(&settings, light);
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

fn fixture_card(state: &str) -> Value {
    let from = json!({
        "label": "y•••a@•••••.••• – SP",
        "target": false,
        "usage": [
            { "label": "5h limit", "used": 2.0, "level": "low", "reset": "3h 6m" },
            { "label": "7d limit", "used": 3.0, "level": "low", "reset": "4d 1h" },
            { "label": "Fable", "used": 0.0, "level": "low", "reset": "4d 1h" }
        ]
    });
    let to = json!({
        "label": "x•••0@•••••.••• – 70",
        "target": true,
        "usage": [
            { "label": "5h limit", "used": null, "level": "unknown", "reset": null },
            { "label": "7d limit", "used": 4.0, "level": "low", "reset": "6d 13h" },
            { "label": "Fable", "used": 0.0, "level": "low", "reset": "6d 13h" }
        ]
    });
    match state {
        "resuming" => json!({
            "id": "demo-resuming",
            "provider": "claude",
            "phase": "resuming",
            "verified": true,
            "dismissible": false,
            "heading": "Switching Claude account",
            "lede": "Your conversation will be ready for your next message.",
            "from": from,
            "to": to,
            "steps": [
                { "label": "Switch account", "state": "done" },
                { "label": "Resume conversation", "state": "active" }
            ]
        }),
        "failed" => json!({
            "id": "demo-failed",
            "provider": "claude",
            "phase": "failed",
            "verified": false,
            "dismissible": true,
            "heading": "Couldn’t complete the switch",
            "lede": "The new account isn’t ready yet.",
            "from": from,
            "to": to,
            "failure": "We couldn’t confirm the new login. Retry this switch or use Switch Account to choose another account.",
            "retry": { "accountId": "acct-2", "busy": false }
        }),
        _ => json!({
            "id": "demo-switching",
            "provider": "claude",
            "phase": "switching",
            "verified": false,
            "dismissible": false,
            "heading": "Switching Claude account",
            "lede": "Your conversation will be ready for your next message.",
            "from": from,
            "to": to,
            "steps": [
                { "label": "Switch account", "state": "active" },
                { "label": "Resume conversation", "state": "pending" }
            ]
        }),
    }
}

/// Realistic mock of the chat composer and pill strip (same tokens as `composer.rs` /
/// `pills_strip.rs`) so the account-switch card can be inspected directly beside it over the
/// wallpaper.
fn demo_composer(look: &AccountSwitchCardLook) -> AnyElement {
    let strip_tint = if look.light {
        gpui::black().opacity(0.035)
    } else {
        gpui::white().opacity(0.035)
    };
    let pill = |label: &str| {
        div()
            .h(px(25.0))
            .px(px(9.0))
            .flex()
            .items_center()
            .gap(px(5.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(look.composer_border)
            .bg(strip_tint)
            .text_size(px(12.0))
            .text_color(look.foreground)
            .child(label.to_string())
    };
    div()
        .w(px(740.0))
        .flex()
        .flex_col()
        .rounded(px(22.0))
        .border_1()
        .border_color(look.composer_border)
        .bg(look.composer_background)
        .overflow_hidden()
        .child(
            div()
                .w_full()
                .h(px(36.0))
                .px(px(12.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .bg(strip_tint)
                .border_b_1()
                .border_color(look.composer_border)
                .child(pill("Ghostex"))
                .child(pill("main"))
                .child(div().flex_1())
                .child(pill("Resources"))
                .child(pill("Workers · 11"))
                .child(pill("Autopilot"))
                .child(pill("Stashed 3")),
        )
        .child(
            div()
                .w_full()
                .px(px(16.0))
                .pt(px(14.0))
                .pb(px(12.0))
                .flex()
                .flex_col()
                .gap(px(18.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(look.muted)
                        .child("Send a follow-up message..."),
                )
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .text_size(px(12.5))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(look.foreground)
                                .child(
                                    svg()
                                        .path("agent-icons/claude.svg")
                                        .size(px(14.0))
                                        .text_color(look.brand_color),
                                )
                                .child("Default (recommended)"),
                        )
                        .child(
                            div()
                                .size(px(28.0))
                                .rounded(px(9.0))
                                .bg(if look.light {
                                    look.accent
                                } else {
                                    rgb(0xe5e5e5).into()
                                })
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(13.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(if look.light {
                                    gpui::white()
                                } else {
                                    rgb(0x141518).into()
                                })
                                .child("↑"),
                        ),
                ),
        )
        .into_any_element()
}

impl Render for AccountSwitchPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let look = default_chat_look(self.light, self.glass);
        let card = fixture_card(&self.state);
        let compare = self.state == "compare";
        let wallpaper = (!self.light).then(|| self.art_path.clone()).flatten();

        let mut root = div()
            .relative()
            .size_full()
            .bg(look.background)
            .font_family(look.font.clone());

        // Paint the Polar Night wallpaper with the app's exact `window_art_backdrop` dimming (45%)
        // and work-area tint (67%) so dark previews show real glass over the artwork.
        if let Some(path) = wallpaper {
            root = root
                .child(
                    img(path)
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                )
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .bg(gpui::black().opacity(0.45)),
                )
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .bg(look.background.opacity(0.67)),
                );
        }

        let overlay = render_account_switch_overlay(
            &card,
            PREVIEW_WIDTH,
            &look,
            AccountSwitchCardHooks::default(),
        );

        if compare {
            // Side-by-side comparison: card over the Polar Night wallpaper with the live composer
            // and pill strip right below it at full brightness so surface, glass translucency,
            // border, radius, tiles and text hierarchy compare directly.
            root.child(
                div()
                    .relative()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .pt(px(52.0))
                    .child(
                        div()
                            .w(px(740.0))
                            .flex()
                            .flex_col()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(look.muted)
                                    .child("Worked for 29s · 1 message sent · 1 tool call"),
                            )
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .line_height(px(21.0))
                                    .text_color(look.prose)
                                    .child("Rate limit reached on Claude account SP. Switching to account 70 and resuming the conversation."),
                            ),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .pb(px(190.0))
                    .child(overlay),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(px(32.0))
                    .flex()
                    .justify_center()
                    .child(demo_composer(&look)),
            )
        } else {
            root.child(
                div()
                    .relative()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_between()
                    .py(px(40.0))
                    .child(
                        div()
                            .w(px(740.0))
                            .flex()
                            .flex_col()
                            .gap(px(12.0))
                            .mt(px(40.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(look.muted)
                                    .child("Worked for 29s · 1 message sent · 1 tool call"),
                            )
                            .child(
                                div()
                                    .p(px(16.0))
                                    .rounded(px(14.0))
                                    .border_1()
                                    .border_color(look.composer_border)
                                    .bg(look.composer_background)
                                    .text_size(px(14.0))
                                    .text_color(look.prose)
                                    .child("Rate limit reached on Claude account SP. Switching to account 70 and resuming the conversation."),
                            ),
                    )
                    .child(demo_composer(&look)),
            )
            .child(overlay)
        }
    }
}

pub(crate) fn open(demo: &DemoEnv, cx: &mut App) {
    let state = demo.state.clone();
    let light = demo.palette.light;
    // Default theme ships with window glass enabled (`windowGlass: "frosted"`).
    let glass = std::env::var("GHOSTEX_NATIVE_MODAL_DEMO_GLASS")
        .map(|value| value.trim() != "0")
        .unwrap_or(true);
    let art_path = find_polar_night_art();
    let _ = open_modal_window(
        PREVIEW_WIDTH,
        PREVIEW_HEIGHT,
        move |_, cx| {
            cx.new(|_| AccountSwitchPreview {
                state,
                light,
                glass,
                art_path,
            })
        },
        cx,
    );
}
