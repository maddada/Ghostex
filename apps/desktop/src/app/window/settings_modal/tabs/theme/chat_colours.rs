//! The Chat colours group of the Themes page: Chat theme, a live chat preview, then Your
//! messages (its colour and how see-through it is), Chat box, Cards and Accent, each the theme
//! role's editor (roles.rs) with its Reset. Theme colours leaves these four out.
//!
//! CDXC:Theming 2026-11-18 DECISION:
//! User: "Ex i see chat bubble is this color but i can't change it in ui at all", then Q3980 "all": a Chat colours group high on the Interface page, led by Your messages with a See-through slider and a live preview, then Chat box, Cards and Accent. It sits right under Theme it in your words. "Show each thing once" keeps these four roles out of Theme colours below.
//! SEE-ALSO: packages/settings-catalog/src/theme_roles.rs (the roles and the rest of the decision), roles.rs (the editor).

use super::super::super::fields::{RowSpec, setting_row, settings_section};
use super::chat_preview::chat_colours_preview;
use super::controls::{ICON_CHAT, ICON_COLOURFULNESS, ICON_TRANSPARENCY};
use super::roles::SliderReads;
use super::{ThemeCx, ThemeTab};
use ghostex_settings_catalog::theme_roles::THEME_ROLES_SETTING;
use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div,
    px,
};

const ICON_CHAT_BOX: &str = "modals/settings/keyboard.svg";

/// The group's rows: the role, its title, what it colours, its icon and how its slider reads.
const ROWS: [(&str, &str, &str, &str, SliderReads); 4] = [
    (
        "userMessage",
        "Your messages",
        "Your bubble. Drag See-through so the glass shows through it.",
        ICON_CHAT,
        SliderReads::SeeThrough,
    ),
    (
        "chatBox",
        "Chat box",
        "The box you type in.",
        ICON_CHAT_BOX,
        SliderReads::Opacity,
    ),
    (
        "surface",
        "Cards",
        "Tool calls, questions and the other cards in chat and panels.",
        ICON_TRANSPARENCY,
        SliderReads::Opacity,
    ),
    (
        "accent",
        "Accent",
        "Links, switches and sliders that are on, and other marks of the theme colour.",
        ICON_COLOURFULNESS,
        SliderReads::Opacity,
    ),
];

impl ThemeTab {
    pub(super) fn chat_colours_section(
        &mut self,
        t: &ThemeCx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // Chat theme leads the group (Q4153: "Chat colours: Chat theme, Chat colours preview, …").
        let chat_theme = self.appearance_override_row(t, "sessionChatTheme", window, cx);
        if chat_theme.is_none() && !t.visible(THEME_ROLES_SETTING) {
            return None;
        }
        let p = t.p;
        let mut rows: Vec<AnyElement> = chat_theme.into_iter().collect();
        if !t.visible(THEME_ROLES_SETTING) {
            return settings_section(&p, "Chat colours", None, None, rows)
                .map(IntoElement::into_any_element);
        }
        if !t.searching {
            rows.push(
                div()
                    .w_full()
                    .px(px(20.0))
                    .py(px(14.0))
                    .child(chat_colours_preview(&p, &t.values))
                    .into_any_element(),
            );
        }
        for (role, title, about, icon, reads) in ROWS {
            let (control, reset) = self.role_editor(t, role, reads, window, cx);
            rows.push(setting_row(
                &p,
                format!("chat-colour-{role}"),
                RowSpec::new(title)
                    .description(about)
                    .icon(icon)
                    .modified(reset.is_some())
                    .marker_after_label(),
                reset,
                control,
                cx,
            ));
        }
        settings_section(
            &p,
            "Chat colours",
            Some(SharedString::from(
                "Your messages, the box you type in, cards and the accent. A colour not set keeps the theme's own.",
            )),
            None,
            rows,
        )
        .map(IntoElement::into_any_element)
    }
}
