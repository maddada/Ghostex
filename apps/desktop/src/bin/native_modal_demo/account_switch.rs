//! Account-switch card preview (`GHOSTEX_NATIVE_MODAL_DEMO=account-switch`): draws the chat's
//! own `account_switch_card_view::render_account_switch_overlay` over a default-theme chat surface
//! using Ghostex's built-in default theme tokens (Polar Night Dark / Everforest Light) from
//! `ghostex_settings_catalog`. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): `switching` (default),
//! `resuming`, `settled`, `failed`.

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
    App, AppContext as _, Context, Hsla, IntoElement, ParentElement as _, Render, Rgba, Styled as _,
    Window, div, px, rgb,
};
use serde_json::{Map, Value, json};

const PREVIEW_WIDTH: f32 = 1650.0;
const PREVIEW_HEIGHT: f32 = 962.0;

struct AccountSwitchPreview {
    state: String,
    light: bool,
    glass: bool,
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
    let mut border: Hsla = if glass {
        if light {
            gpui::black().opacity(0.08)
        } else {
            gpui::white().opacity(0.08)
        }
    } else {
        lifted(0.062, 0.095)
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
    if let Some(c) = role_hsla("accent") {
        accent = background.blend(c);
    }

    let fills = neutral_button_fills(light);
    AccountSwitchCardLook {
        background,
        card_panel,
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

impl Render for AccountSwitchPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let look = default_chat_look(self.light, self.glass);
        let card = fixture_card(&self.state);
        let overlay = render_account_switch_overlay(
            &card,
            PREVIEW_WIDTH,
            &look,
            AccountSwitchCardHooks::default(),
        );
        // Simulate a chat pane with a realistic transcript behind the overlay so card surface,
        // backdrop, contrast and shadow read just like the live app.
        div()
            .relative()
            .size_full()
            .bg(look.background)
            .font_family(look.font.clone())
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(20.0))
                    .px(px(120.0))
                    .child(
                        div()
                            .w(px(760.0))
                            .p(px(16.0))
                            .rounded(px(12.0))
                            .border_1()
                            .border_color(look.border)
                            .bg(look.card_panel)
                            .text_size(px(14.0))
                            .text_color(look.prose)
                            .child("Rate limit reached on Claude account SP. Switching to account 70 and resuming the conversation."),
                    )
                    .child(
                        div()
                            .w(px(760.0))
                            .text_size(px(14.0))
                            .line_height(px(22.0))
                            .text_color(look.muted)
                            .child("Checking usage limits across connected Claude accounts and verifying shared conversation history before switching profiles."),
                    ),
            )
            .child(overlay)
    }
}

pub(crate) fn open(demo: &DemoEnv, cx: &mut App) {
    let state = demo.state.clone();
    let light = demo.palette.light;
    // Default theme ships with window glass enabled (`windowGlass: "frosted"`).
    let glass = std::env::var("GHOSTEX_NATIVE_MODAL_DEMO_GLASS")
        .map(|value| value.trim() != "0")
        .unwrap_or(true);
    let _ = open_modal_window(
        PREVIEW_WIDTH,
        PREVIEW_HEIGHT,
        move |_, cx| {
            cx.new(|_| AccountSwitchPreview {
                state,
                light,
                glass,
            })
        },
        cx,
    );
}
