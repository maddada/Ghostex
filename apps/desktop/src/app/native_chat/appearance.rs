use crate::app::helpers::{
    gpui_session_chat_background_rgb, gpui_session_chat_uses_light_theme,
    sidebar_titlebar_pack_rgb, sidebar_titlebar_rgb_channels,
};
use gpui::{Hsla, rgb};

#[derive(Clone)]
pub(crate) struct ChatAppearance {
    pub(crate) background: Hsla,
    pub(crate) foreground: Hsla,
    pub(crate) primary: Hsla,
    pub(crate) control_primary: Hsla,
    pub(crate) control_border: Hsla,
    pub(crate) input_border: Hsla,
    pub(crate) ring: Hsla,
    pub(crate) card_background: Hsla,
    /// The theme accent for this chat's appearance (`theme_accent_for_variant`), used by the account-switch card.
    pub(crate) accent: Hsla,
    /// The status-card shell tones (cards.rs): a panel and a footer band stepped off the background.
    pub(crate) card_panel: Hsla,
    pub(crate) card_footer: Hsla,
    pub(crate) prose: Hsla,
    pub(crate) card_muted: Hsla,
    pub(crate) muted: Hsla,
    pub(crate) border: Hsla,
    pub(crate) input: Hsla,
    pub(crate) composer_border: Hsla,
    pub(crate) composer_background: Hsla,
    /// The tinted menu tone the sidebar's menus use, for this chat's own theme variant.
    menu: Hsla,
    pub(crate) scale: f32,
    pub(crate) font: String,
    pub(crate) light: bool,
    pub(crate) verbose: bool,
    pub(crate) simple: bool,
    pub(crate) file_previews: bool,
    pub(crate) transcript_width: Option<f32>,
}

impl ChatAppearance {
    /// CDXC:Theming 2026-09-23 DECISION:
    /// User: under window glass the chat's bubbles, cards, chips and composer are frosted rather than solid: light washes over the glass, and the composer takes the same wash as the user's message bubble. This supersedes the same day's heavier composer tint. Only the pane inside the main window takes this; the chat's popup windows are opaque and keep their fills.
    pub(crate) fn on_window_glass(mut self, glass: bool) -> Self {
        if !glass {
            return self;
        }
        let wash = |white: bool, alpha: f32| -> Hsla {
            gpui::Hsla::from(rgb(if white { 0xffffff } else { 0x000000 })).opacity(alpha)
        };
        if self.light {
            self.input = wash(false, 0.04);
            self.border = wash(false, 0.08);
            self.card_background = wash(true, 0.5);
            self.card_panel = wash(true, 0.5);
            self.card_footer = wash(true, 0.3);
            self.composer_background = self.input;
            self.composer_border = wash(false, 0.08);
        } else {
            self.input = wash(true, 0.06);
            self.border = wash(true, 0.08);
            self.card_background = gpui::transparent_black();
            self.card_panel = wash(true, 0.06);
            self.card_footer = wash(true, 0.03);
            self.composer_background = self.input;
            self.composer_border = wash(true, 0.08);
        }
        self
    }

    /// CDXC:Theming 2026-09-23 DECISION:
    /// User: the chat's menus and popovers (the model picker, the transcript's Copy menu, the composer's ⋯ menu and the like) take the same tinted colour as the sidebar's menus instead of a fixed grey, and are frosted glass while window glass is on. Their windows blur what is behind them, so the fill only thins.
    pub(crate) fn menu_surface(&self) -> Hsla {
        if crate::app::helpers::window_glass_active() {
            crate::app::helpers::frosted_menu_fill(self.menu)
        } else {
            self.menu
        }
    }

    /// The same tinted menu tone at full strength, for chat popups whose windows are transparent
    /// rather than blurred (the suggestions popover off glass and in the maximized composer), where
    /// a thinned fill would show the transcript through it unblurred.
    pub(crate) fn menu_opaque(&self) -> Hsla {
        self.menu
    }

    /// The `--destructive` tone the React transcript painted failed tool results and failed writes in.
    pub(crate) fn error(&self) -> Hsla {
        rgb(if self.light { 0xc53030 } else { 0xef9999 }).into()
    }

    /// The zoom this pane returns to: a preview host's `previewSettings` when it supplies one (Chat
    /// Lab did, and is deleted), `sessionChatZoomPercent` otherwise. `zoom.rs` layers the
    /// keyboard's temporary override over it.
    pub(crate) fn default_zoom_percent(state: &serde_json::Value) -> f32 {
        let snapshot = crate::shared_settings::shared_sidebar_settings_snapshot();
        Self::settings_zoom_percent(
            state["previewSettings"]
                .as_object()
                .unwrap_or_else(|| snapshot.object()),
        )
    }

    fn settings_zoom_percent(settings: &serde_json::Map<String, serde_json::Value>) -> f32 {
        settings
            .get("sessionChatZoomPercent")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(100.0) as f32
    }

    pub(crate) fn current(state: &serde_json::Value) -> Self {
        let snapshot = crate::shared_settings::shared_sidebar_settings_snapshot();
        let settings = state["previewSettings"]
            .as_object()
            .unwrap_or_else(|| snapshot.object());
        let light = gpui_session_chat_uses_light_theme(settings);
        let color = |dark, light_color| rgb(if light { light_color } else { dark }).into();
        let enabled = |name| settings.get(name).and_then(serde_json::Value::as_bool) == Some(true);
        // The transcript and its cards share one backing tone derived from the chat's own theme
        // variant (the chat may be dark while the app is light), see session_chat_background_for_chrome.
        let background_rgb = gpui_session_chat_background_rgb(settings);
        let background: Hsla = rgb(background_rgb).into();
        /*
        CDXC:Theming 2026-09-22 DECISION:
        User: the user's message bubble and the composer must take from the theme colour too; they were
        fixed grays that never changed. In dark mode the opaque fills are fixed steps toward white off
        the chat background (3% for the bubble/composer/input, 6% for the border, 8% for the composer
        border), which land on the old #141414 / #1c1c1c / #202020 over the neutral #0d0d0d and carry a
        tinted preset's hue otherwise. Light mode does the same with steps toward black (2026-09-22:
        User: "in the light one too"), so a tinted light preset carries into the bubble and composer.
        */
        let toward = |target: f32, amount: f32| -> Hsla {
            let [red, green, blue] = sidebar_titlebar_rgb_channels(background_rgb);
            rgb(sidebar_titlebar_pack_rgb([
                red + (target - red) * amount,
                green + (target - green) * amount,
                blue + (target - blue) * amount,
            ]))
            .into()
        };
        // Dark fills step toward white, light fills toward black, by amounts that land on the old
        // fixed values over the neutral defaults (#141414 / #1c1c1c / #202020 dark, #f4f4f5 /
        // #e4e4e7 / #ebebeb light); the composer and cards in light mode step a little toward white.
        let lifted = |dark_amount: f32, light_amount: f32| -> Hsla {
            if light {
                toward(0.0, light_amount)
            } else {
                toward(255.0, dark_amount)
            }
        };
        let raised = |amount: f32| -> Hsla { toward(255.0, amount) };
        Self {
            accent: rgb(crate::app::helpers::theme_accent_for_variant(
                settings, light,
            ))
            .into(),
            background,
            menu: rgb(
                crate::app::helpers::sidebar_titlebar_menu_background_for_chrome(
                    crate::app::helpers::resolved_custom_sidebar_titlebar_background_for_variant(
                        settings, light,
                    ),
                ),
            )
            .into(),
            foreground: color(0xfcfcfc, 0x27272a),
            primary: color(0xb4b8c0, 0x27272a),
            control_primary: color(0xe5e5e5, 0x18181b),
            control_border: if light {
                toward(0.0, 0.095)
            } else {
                gpui::Hsla::from(rgb(0xffffff)).opacity(0.06)
            },
            input_border: if light {
                toward(0.0, 0.095)
            } else {
                gpui::Hsla::from(rgb(0xffffff)).opacity(0.08)
            },
            card_background: if light { raised(0.3) } else { background },
            // CDXC:Theming 2026-09-22 DECISION: User: the cards must take the theme's tint in the blueish light theme; they were not tinted at all. Light mode mixes toward white off the chat background (60% for the panel, 10% for the footer band) instead of the fixed #fdfdfd / #f5f5f5 pinned on 2026-09-16, which this supersedes, keeping the cards lighter than the page.
            // Over the neutral defaults these are the old #1e1e1e / #151515 dark and #fbfbfb / #f5f5f6
            // light tones.
            card_panel: if light {
                raised(0.6)
            } else {
                lifted(0.07, 0.0)
            },
            card_footer: if light {
                raised(0.1)
            } else {
                lifted(0.033, 0.0)
            },
            ring: color(0x737373, 0x9f9fa9),
            prose: color(0xb4b8c0, 0x4d4d50),
            // CDXC:SessionChat 2026-09-13 DECISION: User: fix unreadable chat cards in light mode through the shared theme. Dark mode keeps #fcfcfc titles and actions (`foreground`) and #b4b8bf content (`card_muted`); light mode uses the light palette; sizes, weights and outlined actions stay the same.
            card_muted: color(0xb4b8bf, 0x71717b),
            muted: color(0x9e9e9e, 0x71717b),
            border: lifted(0.062, 0.095),
            input: lifted(0.03, 0.032),
            composer_border: lifted(0.08, 0.067),
            composer_background: if light {
                raised(0.3)
            } else {
                lifted(0.03, 0.0)
            },
            scale: super::zoom::keyboard_zoom_percent(state)
                .unwrap_or_else(|| Self::settings_zoom_percent(settings))
                / 100.0,
            font: settings
                .get("sessionChatFontFamily")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("DM Sans")
                .to_string(),
            light,
            verbose: state["verboseOverride"]
                .as_bool()
                .unwrap_or_else(|| enabled("sessionChatVerboseMode")),
            simple: enabled("sessionChatSimpleMode"),
            file_previews: enabled("sessionChatFileEditPreviews"),
            transcript_width: enabled("sessionChatCustomTranscriptWidthEnabled").then(|| {
                settings
                    .get("sessionChatTranscriptWidthPercent")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(75.0) as f32
                    / 100.0
            }),
        }
    }
}
