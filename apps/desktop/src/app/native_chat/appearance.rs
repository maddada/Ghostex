use super::chat_text_look::{
    CHAT_BODY_FONT, ChatTextColors, ChatTextRoles, INPUT_STEP_DARK, INPUT_STEP_LIGHT,
    user_bubble_blue, zoom_scale,
};
use crate::app::helpers::{
    gpui_session_chat_uses_light_theme, sidebar_titlebar_pack_rgb, sidebar_titlebar_rgb_channels,
    work_area_background_for_variant,
};
use ghostex_gx_chat_core::transcript::chat_look::ChatLook;
use gpui::{Hsla, Styled as _, rgb};
use serde::Deserialize as _;

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
    /// The rows and boxes inside a card (a question's options, its note box, a page card's window):
    /// darker than the panel in dark mode, the input tone in light mode.
    pub(crate) card_row: Hsla,
    pub(crate) prose: Hsla,
    /// A fenced code block's text: this lane's prose tone before Text strength, which leaves code
    /// where it is.
    pub(crate) code_text: Hsla,
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
    /// The Normal look (Simple renamed), which Summary shows too: tool runs and file edits fold
    /// behind one-line labels.
    pub(crate) simple: bool,
    /// Live page cards start folded to a single compact row: on in Summary view
    /// (`collapsePageCards`, gx-chat-core `transcript::view_level::collapse_page_cards`).
    pub(crate) collapse_page_cards: bool,
    /// The folds under each agent message (a reply's tool calls, a thought's tools, a finished
    /// turn's "Worked" log) start open: in Detailed view.
    ///
    /// CDXC:SessionChat 2026-10-14 DECISION: User: "if the user disables simple mode we should expand what's under the areas under each agent message by default". In Detailed view (Simple mode off, now) the work under each reply starts open; closing one sticks as a manual toggle does, each tool row's own output stays closed, and Normal keeps its folds. Verbose, which opened them too, is retired (gx-chat-core transcript/view_level.rs).
    /// SEE-ALSO: apps/mobile/app/src/chat/native/transcript/Transcript.tsx (`workOpen`).
    /// Settings > Extensions > Experimental > Tool call lines: a tool card draws as one quiet line
    /// with its calls as rows under it (`tool_card.rs`).
    pub(crate) tool_lines: bool,
    pub(crate) work_open: bool,
    pub(crate) file_previews: bool,
    /// Grey thinking (`sessionChatMutedThinking`): thinking takes the muted text colour, so it reads
    /// apart from a reply (`thinking_lane`); how bright a reply reads is Text strength's alone.
    pub(crate) muted_thinking: bool,
    pub(crate) transcript_width: Option<f32>,
    /// How marked sections in agents' replies are drawn.
    pub(crate) marked_sections: super::marked_sections::MarkerStyle,
    /// Handover heading color strength, 0 to 1, from the document's `markedHeadingStrength`
    /// (absent: 60%, the default).
    pub(crate) marked_heading_strength: f32,
    /// Text strength, 0 to 100, from the document's `chatTextStrength` (absent: 0%, the default).
    pub(crate) text_strength: u8,
    /// Whether this chat is drawn on window glass (`on_window_glass`).
    pub(crate) window_glass: bool,
    /// The chat look settings, from the document's `chatLook` (absent: all Default, today's chat).
    pub(crate) chat_look: ChatLook,
    /// Chat box matches transcript width (`chatComposerMatchesTranscriptWidth`, desktop only): the
    /// chat box takes the transcript column's width (composer.rs).
    pub(crate) composer_matches_transcript: bool,
    /// Your message's fill under the Band style: one input step further off the background than
    /// the bubble (`input`).
    pub(crate) user_band: Hsla,
    /// Your message bubble's fill (and your `!` command card's): a soft blue over the chat
    /// background, unless the Your messages theme role is set.
    ///
    /// CDXC:SessionChat 2026-11-15 DECISION (issue #618, Q3270 A): "my own message bubble is nearly the same grey as the background"; the user picked stronger headings plus a soft blue bubble for their own messages, on the phone and then the desktop. Blue #4a90d9 at 30% over a dark chat, #3b82f6 at 12% over a light one; under window glass the same blues as see-through washes, never a black fill.
    /// SEE-ALSO: apps/mobile/app/src/chat/native/transcript/theme.ts (`userBubble`).
    pub(crate) user_message: Hsla,
    /// A lane's own heading colour (Grey thinking's muted tone); `None` draws headings and bold
    /// words in the bright `foreground` (`markdown_style::text_style`).
    pub(crate) lane_heading: Option<Hsla>,
}

impl ChatAppearance {
    /// CDXC:Theming 2026-09-23 DECISION:
    /// User: under window glass the chat's bubbles, cards, chips and composer are frosted rather than solid: light washes over the glass, and the composer takes the same wash as the user's message bubble. This supersedes the same day's heavier composer tint. Only the pane inside the main window takes this; the chat's popup windows are opaque and keep their fills.
    pub(crate) fn on_window_glass(mut self, glass: bool) -> Self {
        if !glass {
            return self;
        }
        self.window_glass = true;
        let wash = |white: bool, alpha: f32| -> Hsla {
            gpui::Hsla::from(rgb(if white { 0xffffff } else { 0x000000 })).opacity(alpha)
        };
        // Your bubble keeps its blue as a see-through wash, so the glass shows through it.
        let (blue, share) = user_bubble_blue(self.light);
        self.user_message = gpui::Hsla::from(rgb(blue)).opacity(share);
        if self.light {
            self.input = wash(false, 0.04);
            self.user_band = wash(false, 0.08);
            self.border = wash(false, 0.08);
            self.card_background = wash(true, 0.5);
            self.card_panel = wash(true, 0.5);
            self.card_footer = wash(true, 0.3);
            // The Inbox card's wash, as without glass (CDXC:SessionChat 2026-11-25 on
            // `composer_background` in `derive`); dark mode's input wash already matches it.
            self.composer_background = self.card_panel;
            self.composer_border = wash(false, 0.08);
            self.card_row = self.input;
        } else {
            self.input = wash(true, 0.06);
            self.user_band = wash(true, 0.12);
            self.border = wash(true, 0.08);
            self.card_background = gpui::transparent_black();
            self.card_panel = wash(true, 0.06);
            self.card_footer = wash(true, 0.03);
            self.composer_background = self.input;
            self.composer_border = wash(true, 0.08);
            self.card_row = self.input;
        }
        self.with_theme_roles()
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

    /// The fill of a card the chat draws over its own content inside the window (the Dictate card
    /// and pill over the chat box, the subagent viewer over the transcript). Under window glass it
    /// is the chat's colour lifted toward white and kept opaque (`glass_lifted_opaque_from`); glass
    /// off, `solid`.
    ///
    /// CDXC:Theming 2026-11-20 DECISION:
    /// User, of the "Hex isn't installed" card on Arch (Hyprland) with the chat box's placeholder and
    /// buttons showing through it: "the bg shown here isn't frosted at all". Shown the options
    /// (Q4293), they picked A, an opaque lifted fill. No platform blurs one part of a window over
    /// another (macOS's material, Windows' acrylic and a Linux compositor's blur all blur behind
    /// the whole window), so these cards took the transcript cards' 6% wash (`card_panel`,
    /// `composer_background`) with the box's text sharp under it, on every platform. They take the
    /// lift Q4247 gave the chat's other floating surfaces, opaque rather than 96% because text sits
    /// right under them.
    pub(crate) fn over_content_fill(&self, solid: Hsla) -> Hsla {
        if self.window_glass {
            crate::app::helpers::glass_lifted_opaque_from(self.background, self.light)
        } else {
            solid
        }
    }

    /// The `--destructive` tone the React transcript painted failed tool results and failed writes in.
    /// The Danger theme role when it is set (helpers/theme_roles.rs).
    pub(crate) fn error(&self) -> Hsla {
        crate::app::helpers::theme_role_or("danger", self.light, Self::own_error(self.light))
    }

    /// [`Self::error`] while the Danger role is not set.
    pub(crate) fn own_error(light: bool) -> Hsla {
        rgb(if light { 0xc53030 } else { 0xef9999 }).into()
    }

    /// A finished tool call's check (the 2026-10-11 work look, gx-chat-core `work_summary.rs`).
    pub(crate) fn success(&self) -> Hsla {
        crate::app::helpers::theme_role_or("success", self.light, Self::own_success(self.light))
    }

    /// [`Self::success`] while the Success role is not set.
    pub(crate) fn own_success(light: bool) -> Hsla {
        rgb(if light { 0x059669 } else { 0x34d399 }).into()
    }

    /// The look of the agent's thinking: a thought's prose, its lane marker and a reasoning
    /// headline. Light mode always mutes it; dark mode draws it like the answer unless Grey
    /// thinking is on.
    ///
    /// CDXC:SessionChat 2026-10-16 DECISION: User, on a tester's request to draw the agent's thinking grey and its output white as terminals do: "Sure please do it as a toggle (disabled by default) so we can check how it looks." With `sessionChatMutedThinking` on, thinking takes the theme's muted text colour (`muted`) and an agent's replies its full foreground, so the pair reads apart in dark and light themes; off keeps the unified look. Superseded for replies on 2026-11-09 (`ChatTextColors::reply_base`): the reply's brightness is Text strength's alone.
    /// SEE-ALSO: apps/mobile/app/src/chat/native/transcript/MessageRow.tsx (`thinkingTone`, the reply's `proseColor`), packages/settings-catalog/src/data/defaults.rs.
    pub(crate) fn thinking_lane(&self) -> Self {
        let tone = self.text_colors().thinking(self.muted_thinking);
        let mut lane = self.clone();
        lane.prose = tone;
        lane.code_text = tone;
        if self.muted_thinking {
            // A thought's headings and a reasoning headline go grey with its prose, and its bold
            // words keep the prose's colour.
            lane.primary = tone;
            lane.lane_heading = Some(tone);
        }
        lane
    }

    /// The look of an agent's reply prose, raised by Text strength (`ChatTextColors::reply_base`
    /// says why Grey thinking no longer lifts it). Unchanged at the default 0%.
    pub(crate) fn reply_lane(&self) -> Option<Self> {
        (self.text_strength > 0).then(|| {
            let mut lane = self.clone();
            lane.prose = self.strengthened(self.prose);
            // A reply's headings stay at least as bright as its prose.
            lane.primary = self.strengthened(self.primary);
            lane
        })
    }

    /// A body text colour at this chat's Text strength (CDXC:SessionChat 2026-10-31 in
    /// packages/gx-chat-core/src/transcript/text_strength.rs).
    pub(crate) fn strengthened(&self, color: Hsla) -> Hsla {
        self.text_colors().strengthened(color, self.text_strength)
    }

    /// Your message's text under a Your messages style, at this chat's Text strength.
    pub(crate) fn user_style_prose(&self) -> Hsla {
        self.text_colors().user_style_prose(self.text_strength)
    }

    /// This chat's body text colours (`chat_text_look.rs`, which the Settings preview draws from).
    fn text_colors(&self) -> ChatTextColors {
        ChatTextColors {
            foreground: self.foreground,
            primary: self.primary,
            prose: self.prose,
            muted: self.muted,
            light: self.light,
        }
    }

    pub(crate) fn current(state: &serde_json::Value) -> Self {
        let snapshot = crate::shared_settings::shared_sidebar_settings_snapshot();
        Self::resolve(state, gpui_session_chat_uses_light_theme(snapshot.object()))
    }

    /// The chat's look in one appearance, for chat content drawn outside a chat: the app's
    /// Markdown table popup follows the app's appearance, which can differ from the chat theme.
    pub(crate) fn for_variant(light: bool) -> Self {
        Self::resolve(&serde_json::Value::Null, light)
    }

    /// The chat's look in one appearance with no theme role laid over it: what each role paints
    /// while it is not set, which Settings shows on a role's row (helpers/theme_role_defaults.rs).
    pub(crate) fn without_theme_roles(light: bool) -> Self {
        Self::derive(&serde_json::Value::Null, light)
    }

    /// A link's colour while neither the Links nor the Accent role is set.
    pub(crate) fn own_link(light: bool) -> Option<Hsla> {
        super::markdown_links::own_link_color(light)
    }

    fn resolve(state: &serde_json::Value, light: bool) -> Self {
        Self::derive(state, light).with_theme_roles()
    }

    /// The theme roles the user or an agent set (helpers/theme_roles.rs) over the derived colours.
    /// A role not set keeps the colour.
    fn with_theme_roles(mut self) -> Self {
        use crate::app::helpers::{theme_role_or, theme_role_rgba};
        let light = self.light;
        let role = |id: &str| theme_role_rgba(id, light).map(Hsla::from);
        if let Some(border) = role("border") {
            self.border = border;
            self.input_border = border;
            self.control_border = border;
            self.composer_border = border;
        }
        // Body text and your message take their roles where the Settings preview reads them too.
        let text_roles = ChatTextRoles::read(&role);
        let text = self.text_colors().with_roles(text_roles);
        self.foreground = text.foreground;
        self.primary = text.primary;
        self.prose = text.prose;
        self.muted = text.muted;
        if let Some(text) = text_roles.text {
            self.code_text = text;
        }
        // Accent comes through `theme_accent_for_variant`, which lays the role over the work area.
        self.card_panel = theme_role_or("surface", light, self.card_panel);
        if let Some(control) = role("control") {
            self.input = control;
            self.card_row = control;
        }
        if let Some(menu) = role("menu") {
            self.menu = menu;
        }
        self.ring = theme_role_or("focusRing", light, self.ring);
        if let Some(message) = text_roles.user_message {
            self.user_message = message;
            self.user_band = message;
        }
        self.composer_background = theme_role_or("chatBox", light, self.composer_background);
        self
    }

    fn derive(state: &serde_json::Value, light: bool) -> Self {
        let snapshot = crate::shared_settings::shared_sidebar_settings_snapshot();
        let settings = snapshot.object();
        let color = |dark, light_color| rgb(if light { light_color } else { dark }).into();
        let text = ChatTextColors::for_variant(light);
        let enabled = |name| settings.get(name).and_then(serde_json::Value::as_bool) == Some(true);
        // The chat's one view level comes from the core (gx-chat-core transcript/view_level.rs has
        // the user's words): only Detailed drops the Normal look; Summary shows it too.
        let simple = state["viewLevel"].as_str() != Some("detailed");
        // The transcript and its cards share one backing tone derived from the chat's own theme
        // variant (the chat may be dark while the app is light), see session_chat_background_for_chrome.
        let background_rgb = work_area_background_for_variant(settings, light);
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
            foreground: text.foreground,
            primary: text.primary,
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
            // CDXC:Theming 2026-10-11 DECISION: User: the cards must take the theme's tint ("in the blueish light theme; they were not tinted at all", 2026-09-22), and every chat card is as dark as the page question cards ("yellow + dark all", after "background for the whole question component still too light" on the question page). Dark mode lifts the panel 3.5%, the rows inside a card 1.5% and the footer band 2.5% toward white off the chat background, replacing 7% / 3% / 3.3%. Light mode keeps mixing toward white (60% for the panel, 10% for the footer band) and its rows keep the input tone.
            // SEE-ALSO: server/src/visual_pages/kit.rs (the page question cards' `--gx-card`, `--gx-option` and `--gx-footer` follow the same rule), apps/mobile/app/src/chat/native/transcript/theme.ts (`cardPanel`, `cardRow`, `cardFooter`).
            card_panel: if light {
                raised(0.6)
            } else {
                lifted(0.035, 0.0)
            },
            card_footer: if light {
                raised(0.1)
            } else {
                lifted(0.025, 0.0)
            },
            card_row: lifted(0.015, 0.032),
            ring: focus_ring_color(light),
            prose: text.prose,
            code_text: text.prose,
            // CDXC:SessionChat 2026-09-13 DECISION: User: fix unreadable chat cards in light mode through the shared theme. Dark mode keeps #fcfcfc titles and actions (`foreground`) and #b4b8bf content (`card_muted`); light mode uses the light palette; sizes and weights stay the same; card actions are filled (CDXC:AppModal 2026-10-14 on `neutral_button_fills`).
            card_muted: color(0xb4b8bf, 0x71717b),
            muted: text.muted,
            border: lifted(0.062, 0.095),
            input: lifted(INPUT_STEP_DARK, INPUT_STEP_LIGHT),
            user_band: lifted(INPUT_STEP_DARK * 2.0, INPUT_STEP_LIGHT * 2.0),
            user_message: {
                let (tint, share) = user_bubble_blue(light);
                let [red, green, blue] = sidebar_titlebar_rgb_channels(tint);
                let [base_red, base_green, base_blue] =
                    sidebar_titlebar_rgb_channels(background_rgb);
                let mix = |top: f32, base: f32| top * share + base * (1.0 - share);
                rgb(sidebar_titlebar_pack_rgb([
                    mix(red, base_red),
                    mix(green, base_green),
                    mix(blue, base_blue),
                ]))
                .into()
            },
            lane_heading: None,
            composer_border: lifted(0.08, 0.067),
            // CDXC:SessionChat 2026-11-25 DECISION: User, of the chat box under the Inbox: "pls
            // revert the chat box to look similar to the inbox above it ... 2- bg color". Light
            // mode takes the cards' panel tone (`card_panel`, 60% toward white), the Inbox card's,
            // instead of 30%; dark mode was already within half a percent of it and stays.
            composer_background: if light {
                raised(0.6)
            } else {
                lifted(0.03, 0.0)
            },
            scale: zoom_scale(super::zoom::chat_zoom_percent(settings)),
            // A picked app font replaces this through `crate::ui_fonts`, and a saved font that
            // is not installed leaves the chat on it.
            font: CHAT_BODY_FONT.to_string(),
            light,
            simple,
            collapse_page_cards: state["collapsePageCards"] == true,
            work_open: !simple,
            tool_lines: enabled("chatToolCallLines"),
            file_previews: enabled("sessionChatFileEditPreviews"),
            muted_thinking: settings
                .get("sessionChatMutedThinking")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
            marked_sections: super::marked_sections::MarkerStyle::from_settings(settings),
            marked_heading_strength: state["markedHeadingStrength"]
                .as_f64()
                .map_or(0.3, |percent| (percent / 100.0).clamp(0.0, 1.0) as f32),
            text_strength: state["chatTextStrength"]
                .as_f64()
                .and_then(ghostex_gx_chat_core::transcript::text_strength::text_strength)
                .unwrap_or(0),
            window_glass: false,
            // Published only when a setting is off Default, with the effective layout already in
            // `messageLayout` (gx-chat-core transcript/chat_look.rs).
            chat_look: ChatLook::deserialize(&state["chatLook"]).unwrap_or_default(),
            composer_matches_transcript: enabled("chatComposerMatchesTranscriptWidth"),
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

/// A filled chat button's background at rest, under the pointer and while pressed.
#[derive(Clone, Copy)]
pub(crate) struct ChatButtonFills {
    pub(crate) rest: Hsla,
    pub(crate) hover: Hsla,
    pub(crate) pressed: Hsla,
}

impl From<crate::app::window::native_modal_kit::ButtonFills> for ChatButtonFills {
    fn from(fills: crate::app::window::native_modal_kit::ButtonFills) -> Self {
        Self {
            rest: fills.rest.into(),
            hover: fills.hover.into(),
            pressed: fills.pressed.into(),
        }
    }
}

impl ChatAppearance {
    /// The neutral filled button every secondary text action in the chat's cards and dialogs
    /// uses, the same ink wash as the app modals (CDXC:AppModal 2026-10-14 on
    /// `neutral_button_fills`: no border-only buttons).
    pub(crate) fn neutral_button_fills(&self) -> ChatButtonFills {
        crate::app::window::native_modal_kit::neutral_button_fills(self.light).into()
    }
}

/// Paints a chat text button with `fills`: the rest fill, plus the hover and pressed steps while
/// it is enabled. Its border, if any, turns transparent so it keeps a bordered neighbour's size.
/// These set the button's hover style, so the button must not call `hover` itself.
pub(crate) trait ChatFilledButton:
    gpui::StatefulInteractiveElement + gpui::Styled + Sized
{
    fn chat_filled(self, fills: ChatButtonFills, enabled: bool) -> Self {
        self.chat_filled_hover_text(fills, enabled, None)
    }

    /// [`Self::chat_filled`] for a button whose label also brightens to `hover_text` on hover.
    fn chat_filled_hover_text(
        self,
        fills: ChatButtonFills,
        enabled: bool,
        hover_text: Option<Hsla>,
    ) -> Self {
        let button = self.border_color(gpui::transparent_black()).bg(fills.rest);
        if !enabled {
            return button;
        }
        button
            .hover(move |style| {
                let style = style.bg(fills.hover);
                match hover_text {
                    Some(color) => style.text_color(color),
                    None => style,
                }
            })
            .active(move |style| style.bg(fills.pressed))
    }
}

impl ChatFilledButton for gpui::Stateful<gpui::Div> {}

/// The chat composer's focus ring colour, which the sidebar's focus ring wears too
/// (native_sidebar/focus_order.rs).
pub(crate) fn focus_ring_color(light: bool) -> Hsla {
    rgb(if light { 0x9f9fa9 } else { 0x737373 }).into()
}
