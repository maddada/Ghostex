//! The small account pieces the Accounts page and the Extensions page's "Account usage in the
//! sidebar" share: `AccountLogo` (the provider mark in its brand colour), `AccountIdentity` (the
//! logo with the two usage figures), `AccountTitlebarStar`, `CopyCommand`, and Hide emails
//! (`AccountText`).
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{SizedButtonVariant, settings_square_button, tooltip_text};
use super::super::super::palette::SettingsPalette;
use super::data::{Account, UsageWindow, mask_account_text};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, px, svg,
};
use gpui_component::{h_flex, v_flex};

/// Masks or unmasks an input (`type=password`), once per change: `known` remembers what each
/// input was last set to.
pub(crate) fn sync_masked(
    known: &mut std::collections::HashMap<SharedString, bool>,
    id: &SharedString,
    input: &gpui::Entity<gpui_component::input::InputState>,
    masked: bool,
    window: &mut Window,
    cx: &mut gpui::App,
) {
    if known.get(id).copied().unwrap_or(false) == masked {
        return;
    }
    known.insert(id.clone(), masked);
    input.update(cx, |input, cx| input.set_masked(masked, window, cx));
}

/// `AccountText`: the text as shown, masked while Hide emails is on.
pub(crate) fn account_text(text: &str, hide: bool) -> String {
    if hide {
        mask_account_text(text)
    } else {
        text.to_string()
    }
}

/// `AccountLogo`: Claude in its brand orange, Codex in white (dark) or the foreground (light).
///
/// CDXC:AgentProviders 2026-09-08 DECISION:
/// Usage bars remain neutral and selection uses brighter controls. Original provider logos keep their appearance. Account labels sit over larger session icons; sidebar icons have no account indicator.
pub(crate) fn account_logo(p: &SettingsPalette, provider: &str, size: f32) -> AnyElement {
    let ink = if p.light {
        p.foreground
    } else {
        gpui::rgb(0xffffff)
    };
    let (path, color) = match provider {
        "codex" => ("agent-icons/codex.svg", ink),
        "cursor" => ("agent-icons/cursor-cli.svg", ink),
        "copilot" => ("agent-icons/copilot.svg", ink),
        "gemini" => ("agent-icons/gemini.svg", gpui::rgb(0x8b9aff)),
        _ => ("agent-icons/claude.svg", gpui::rgb(0xd97757)),
    };
    svg()
        .path(path)
        .flex_shrink_0()
        .size(px(size))
        .text_color(hsla(color))
        .into_any_element()
}

/// `AccountIdentity`: the logo and the two stacked figures (the second at 60%).
pub(crate) fn account_identity(p: &SettingsPalette, account: &Account) -> AnyElement {
    let [first, second] = account.figures();
    let figure = |id: String, label: Option<String>, value: String, faded: bool| {
        div()
            .id(SharedString::from(id))
            .when(faded, |this| this.opacity(0.6))
            .when_some(label, |this, label| this.tooltip(tooltip_text(label)))
            .child(value)
    };
    let id = account.id();
    h_flex()
        .flex_shrink_0()
        .min_w(px(62.0))
        .items_center()
        .gap(px(7.0))
        .child(account_logo(p, &account.provider(), 21.0))
        .child(
            v_flex()
                .font_family(MODAL_MONO_FONT)
                .text_size(px(11.0))
                .line_height(px(14.3))
                .text_color(hsla(p.foreground))
                .child(figure(
                    format!("account-{id}-figure-1"),
                    first.0,
                    first.1,
                    false,
                ))
                .child(figure(
                    format!("account-{id}-figure-2"),
                    second.0,
                    second.1,
                    true,
                )),
        )
        .into_any_element()
}

/// `AccountTitlebarStar`: shows or hides the account's usage in the sidebar, on the Extensions
/// page's Account usage list. The Accounts page uses a switch instead (`render_account_detail`).
///
/// CDXC:AgentProviders 2026-10-14 DECISION:
/// User picked "Replace the star with a Show usage in the sidebar switch" for Settings > Accounts: each account's detail pane has a Show usage in the sidebar switch and the account list shows no star. Supersedes the 2026-09-08 star on the Accounts page; the Extensions page's Account usage list keeps this star, with the "in the sidebar" tooltip.
pub(crate) fn account_star<V: 'static>(
    p: &SettingsPalette,
    account: &Account,
    busy: bool,
    on_toggle: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let shown = account.show_in_titlebar();
    let label = if shown {
        "Hide this account's stats from the sidebar"
    } else {
        "Show this account's stats in the sidebar"
    };
    settings_square_button(
        p,
        SharedString::from(format!("account-{}-star", account.id())),
        if shown {
            "modals/settings/star-filled.svg"
        } else {
            "modals/settings/star.svg"
        },
        None,
        SizedButtonVariant::Ghost,
        28.0,
        Some(label.into()),
        busy,
        Some(label.into()),
        on_toggle,
        cx,
    )
}

/// `CopyCommand`: the command in a box with a copy button that shows a check for 1.8s.
pub(crate) fn copy_command<V: 'static>(
    p: &SettingsPalette,
    id: impl Into<SharedString>,
    command: &str,
    copied: bool,
    on_copy: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let id: SharedString = id.into();
    let tooltip: SharedString = if copied {
        "Copied".into()
    } else {
        "Copy command".into()
    };
    h_flex()
        .w_full()
        .items_center()
        .gap(px(12.0))
        .px(px(10.0))
        .py(px(8.0))
        .rounded(px(7.0))
        .border_1()
        .border_color(hsla(p.hairline))
        .bg(hsla(if p.glass {
            p.modal.solid_surface
        } else {
            p.surface
        }))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .font_family(MODAL_MONO_FONT)
                .text_size(px(11.0))
                .line_height(px(16.0))
                .text_color(hsla(p.foreground))
                .child(command.to_string()),
        )
        .child(settings_square_button(
            p,
            SharedString::from(format!("{id}-copy")),
            if copied {
                super::super::super::fields::CHECK_ICON
            } else {
                "modals/settings/copy.svg"
            },
            None,
            SizedButtonVariant::Ghost,
            28.0,
            Some(tooltip),
            false,
            None,
            on_copy,
            cx,
        ))
        .into_any_element()
}

/// `.gx-account-mark` with its indicator: the provider logo at 30% under the badge label (Codex in
/// `#7db8fb`); a `-` indicator shows the plain logo. `scale` 1.0 is the sidebar's 19.2px mark.
///
/// CDXC:AgentProviders 2026-09-09 DECISION:
/// User wants the same account-label font centered over a larger provider icon as its background. Account icons always use original provider colors; labels and adjacent usage figures share the chat indicator's monospace font. Claude keeps its label color; Codex uses #7db8fb. The background icon is 19.2px (20% smaller) and the label is 9.9px (10% bigger). This replaces the label-underneath design. Sidebar icons and account menus have no indicator. Labels accept up to two letters or digits; - hides the label and an empty setting uses the slot number.
pub(crate) fn account_mark(
    p: &SettingsPalette,
    provider: &str,
    indicator: &str,
    scale: f32,
) -> AnyElement {
    if indicator == "-" {
        return account_logo(p, provider, 21.0 * scale);
    }
    let size = 19.2 * scale;
    div()
        .relative()
        .flex_shrink_0()
        .size(px(size))
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .absolute()
                .inset_0()
                .opacity(0.3)
                .child(account_logo(p, provider, size)),
        )
        .child(
            div()
                .font_family(MODAL_MONO_FONT)
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_size(px(9.9 * scale))
                .line_height(px(9.9 * scale))
                .text_color(hsla(if provider == "codex" {
                    gpui::rgb(0x7db8fb)
                } else {
                    p.muted
                }))
                .child(indicator.to_string()),
        )
        .into_any_element()
}

/// The badge an account's sessions show: its indicator, or its slot number when it has none.
pub(crate) fn account_badge(account: &Account) -> String {
    let indicator = account.indicator();
    if indicator.is_empty() {
        account.selector()
    } else {
        indicator
    }
}

/// A neutral usage bar (CDXC:AgentProviders 2026-09-08: usage bars remain neutral).
pub(crate) fn usage_bar(p: &SettingsPalette, used_percent: f64, height: f32) -> AnyElement {
    let fraction = (used_percent / 100.0).clamp(0.0, 1.0) as f32;
    div()
        .flex_1()
        .min_w(px(24.0))
        .h(px(height))
        .rounded(px(height / 2.0))
        .bg(hsla(p.foreground_alpha(0.1)))
        .overflow_hidden()
        .child(
            div()
                .h_full()
                .w(gpui::relative(fraction))
                .rounded(px(height / 2.0))
                .bg(hsla(p.foreground_alpha(0.6))),
        )
        .into_any_element()
}

/// One usage window as a row: its label, the bar, and "41% used · resets in 2h 14m".
pub(crate) fn usage_line(p: &SettingsPalette, window: &UsageWindow) -> AnyElement {
    let label = window.short_label();
    h_flex()
        .w_full()
        .items_center()
        .gap(px(10.0))
        .text_size(px(12.0))
        .line_height(px(16.0))
        .child(
            div()
                .w(px(56.0))
                .flex_shrink_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_color(hsla(p.muted))
                .child(label),
        )
        .child(usage_bar(p, window.used_percent, 6.0))
        .child(
            h_flex()
                .flex_shrink_0()
                .gap(px(4.0))
                .whitespace_nowrap()
                .child(
                    div()
                        .text_color(hsla(p.foreground))
                        .child(format!("{}% used", window.used_percent.round() as i64)),
                )
                .children(
                    window
                        .reset_text()
                        .map(|reset| div().text_color(hsla(p.muted)).child(format!("· {reset}"))),
                ),
        )
        .into_any_element()
}

/// The tone of a `status_tag`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TagTone {
    /// A grey dot: Auto-switch off.
    Off,
    /// A green dot: Auto-switch on.
    On,
    /// Amber: Needs sign-in.
    Warning,
    /// Green: Connected.
    Success,
    /// No dot (saved resets).
    Plain,
}

/// A small rounded tag with an optional status dot ("Auto-switch on", "Needs sign-in").
pub(crate) fn status_tag(
    p: &SettingsPalette,
    text: impl Into<SharedString>,
    tone: TagTone,
) -> AnyElement {
    let (background, foreground, dot) = match tone {
        TagTone::Warning => (
            modal_rgba(0xfbbf24, 0.13),
            if p.light {
                gpui::rgb(0xb45309)
            } else {
                gpui::rgb(0xfcd34d)
            },
            Some(modal_rgba(0xfbbf24, 0.95)),
        ),
        TagTone::Success => (
            modal_rgba(0x34d399, 0.14),
            if p.light {
                gpui::rgb(0x047857)
            } else {
                gpui::rgb(0x6ee7b7)
            },
            Some(modal_rgba(0x34d399, 0.95)),
        ),
        TagTone::On => (
            p.foreground_alpha(0.07),
            p.muted,
            Some(modal_rgba(0x34d399, 0.9)),
        ),
        TagTone::Off => (
            p.foreground_alpha(0.07),
            p.muted,
            Some(p.foreground_alpha(0.3)),
        ),
        TagTone::Plain => (p.foreground_alpha(0.07), p.muted, None),
    };
    h_flex()
        .flex_shrink_0()
        .h(px(22.0))
        .px(px(8.0))
        .gap(px(6.0))
        .items_center()
        .rounded_full()
        .bg(hsla(background))
        .text_size(px(11.5))
        .line_height(px(14.0))
        .whitespace_nowrap()
        .text_color(hsla(foreground))
        .children(dot.map(|dot| div().size(px(6.0)).rounded_full().bg(hsla(dot))))
        .child(text.into())
        .into_any_element()
}
