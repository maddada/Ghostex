//! The Hub's small building blocks, drawn from the modal kit's controls with the Hub's own
//! tokens: the three shadcn `Button` variants it uses, icons, agent logos, status dots, pills
//! and tinted icon tiles (packages/core-ui/agents-hub-sync/shared.tsx (deleted 2026-10-01)).
use super::super::native_modal_kit::{
    ModalButtonSkin, ModalFieldSkin, hsla, modal_skinned_button, modal_text_input_skinned,
    neutral_button_fills, rgba_of, transparent,
};
use super::palette::HubPalette;
use super::sync_model::SyncTone;
use gpui::{
    AnyElement, App, Context, Div, FontWeight, IntoElement, ParentElement as _, Rgba, SharedString,
    Stateful, Styled as _, Window, div, img, px, rgb, svg,
};
use gpui_component::input::InputState;

pub(crate) const ICON_CHEVRON_DOWN: &str = "modals/agents-hub/chevron-down.svg";
pub(crate) const ICON_CHEVRON_RIGHT: &str = "modals/agents-hub/chevron-right.svg";
pub(crate) const ICON_FILE: &str = "modals/agents-hub/file.svg";
pub(crate) const ICON_COPY: &str = "modals/agents-hub/copy.svg";
pub(crate) const ICON_CHECK: &str = "modals/agents-hub/check.svg";
pub(crate) const ICON_CHECK_STRONG: &str = "modals/agents-hub/check-strong.svg";
pub(crate) const ICON_CHECK_HEAVY: &str = "modals/agents-hub/check-heavy.svg";
pub(crate) const ICON_FOLDER_OPEN: &str = "modals/agents-hub/folder-open.svg";
pub(crate) const ICON_EDIT: &str = "modals/agents-hub/edit.svg";
pub(crate) const ICON_REFRESH: &str = "modals/agents-hub/refresh.svg";
/// The Files view's wrap toggle icon.
pub(crate) const ICON_TEXT_WRAP: &str = "modals/git-commit/text-wrap.svg";
pub(crate) const ICON_SAVE: &str = "modals/agents-hub/device-floppy.svg";
pub(crate) const ICON_LAYOUT_GRID: &str = "modals/agents-hub/layout-grid.svg";
pub(crate) const ICON_PLUS: &str = "modals/agents-hub/plus.svg";
pub(crate) const ICON_BOOK: &str = "modals/agents-hub/book-2.svg";
pub(crate) const ICON_CIRCLE_CHECK: &str = "modals/agents-hub/circle-check.svg";
pub(crate) const ICON_EXTERNAL_LINK: &str = "modals/agents-hub/external-link.svg";
pub(crate) const ICON_FILE_TEXT: &str = "modals/agents-hub/file-text.svg";
pub(crate) const ICON_FOLDER: &str = "modals/agents-hub/folder.svg";
pub(crate) const ICON_INFO: &str = "modals/agents-hub/info-circle.svg";
pub(crate) const ICON_LOCK: &str = "modals/agents-hub/lock.svg";
pub(crate) const ICON_TERMINAL: &str = "modals/agents-hub/terminal.svg";
pub(crate) const ICON_ALERT: &str = "modals/agents-hub/alert-triangle.svg";
pub(crate) const ICON_LINK_OFF: &str = "modals/agents-hub/link-off.svg";
pub(crate) const ICON_LINK: &str = "modals/agents-hub/link.svg";

pub(crate) fn icon(path: &'static str, size: f32, color: Rgba) -> AnyElement {
    svg()
        .path(path)
        .size(px(size))
        .flex_shrink_0()
        .text_color(hsla(color))
        .into_any_element()
}

/// The shadcn `Button` variants the Hub renders inside `.ghostex-settings-shadcn`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum HubButtonVariant {
    /// `variant='default'`, which the Settings skin turns into the quiet bordered button.
    Quiet,
    Outline,
    Ghost,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum HubButtonSize {
    /// `h-8 px-3 gap-1.5`.
    Default,
    /// `h-7 px-3 gap-1`.
    Small,
}

fn hub_button_skin(hp: &HubPalette, variant: HubButtonVariant) -> ModalButtonSkin {
    match variant {
        // Every labelled Hub button is filled (CDXC:AppModal 2026-10-14 on `neutral_button_fills`).
        HubButtonVariant::Quiet | HubButtonVariant::Outline => {
            let fills = neutral_button_fills(hp.light);
            ModalButtonSkin {
                background: fills.rest,
                border: rgba_of(hp.background, 0.0),
                text: hp.foreground,
                hover_background: fills.hover,
                hover_text: hp.foreground,
                pressed_background: fills.pressed,
            }
        }
        HubButtonVariant::Ghost => ModalButtonSkin {
            background: rgba_of(hp.background, 0.0),
            border: rgba_of(hp.background, 0.0),
            text: hp.foreground,
            hover_background: hp.muted_fill,
            hover_text: hp.foreground,
            pressed_background: hp.muted_fill,
        },
    }
}

/// A Hub button: `leading_icon` is a 14px (sized) or 16px (icon-only) glyph in the label tone.
#[allow(clippy::too_many_arguments)]
pub(crate) fn hub_button<V: 'static>(
    hp: &HubPalette,
    id: impl Into<gpui::ElementId>,
    variant: HubButtonVariant,
    size: HubButtonSize,
    leading_icon: Option<&'static str>,
    label: Option<SharedString>,
    disabled: bool,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    // A labelled button is always filled (CDXC:AppModal 2026-10-14 on `neutral_button_fills`).
    let variant = match variant {
        HubButtonVariant::Ghost if label.is_some() => HubButtonVariant::Outline,
        other => other,
    };
    let skin = hub_button_skin(hp, variant);
    let (height, padding_x, gap, icon_size) = match size {
        HubButtonSize::Default => (
            32.0,
            if leading_icon.is_some() { 10.0 } else { 12.0 },
            6.0,
            14.0,
        ),
        HubButtonSize::Small => (
            28.0,
            if leading_icon.is_some() { 8.0 } else { 12.0 },
            4.0,
            14.0,
        ),
    };
    modal_skinned_button(
        id,
        skin,
        height,
        padding_x,
        gap,
        leading_icon.map(|path| icon(path, icon_size, skin.text)),
        label,
        disabled,
        0.5,
        on_click,
        cx,
    )
}

/// The Hub's search fields: the shadcn `Input` on the raised card tone with a 13px value.
pub(crate) fn hub_search_input(
    hp: &HubPalette,
    state: &gpui::Entity<InputState>,
    window: &Window,
    cx: &App,
) -> AnyElement {
    modal_text_input_skinned(
        &hp.modal,
        state,
        ModalFieldSkin {
            background: hp.raised,
            border: hp.line,
            focus_border: hp.focus_border,
            text_size: 13.0,
        },
        10.0,
        window,
        cx,
    )
}

/// The brand tint an agent logo mask takes (`AGENT_LOGO_COLORS` in packages/core-ui/agent-logos.ts (deleted 2026-10-01));
/// the white and near-white logos, Codex and Z.ai take the foreground in light themes.
pub(crate) fn agent_logo_color(icon: &str, hp: &HubPalette) -> Rgba {
    let brand = match icon {
        "antigravity-cli" => 0x749bff,
        "browser" => 0x82b7ff,
        "claude" => 0xd97757,
        "codebuddy" => 0x72d6ff,
        "command-code" => 0x22d3ee,
        "cursor-cli" => 0xedecec,
        "devin" => 0x3ea6ff,
        "dsh" => 0x4d6bfe,
        "empryo" => 0x1fa31d,
        "factory-droid" => 0xff7a1a,
        "gemini" => 0x8b9aff,
        "hermes-agent" => 0xf3c46b,
        "kimi" => 0x7b6cf6,
        "kiro" => 0xa6e3ff,
        "omp" => 0xa663ed,
        "openclaude" => 0xf0a68a,
        "opencode" => 0x6d96c0,
        "pi" => 0xc8ff62,
        "qoder" => 0xa991ff,
        "rovo-dev" => 0x4fc3a1,
        _ => 0xffffff,
    };
    if hp.light && matches!(brand, 0xffffff | 0xedecec) {
        return if icon == "zcode" {
            rgb(0x000000)
        } else {
            hp.foreground
        };
    }
    rgb(brand)
}

const AGENT_ICONS: [&str; 27] = [
    "amp-cli",
    "antigravity-cli",
    "browser",
    "claude",
    "codebuddy",
    "command-code",
    "cursor-cli",
    "codex",
    "copilot",
    "devin",
    "dsh",
    "empryo",
    "factory-droid",
    "freebuff",
    "gemini",
    "grok-build",
    "hermes-agent",
    "mastra",
    "kimi",
    "kiro",
    "omp",
    "openclaude",
    "opencode",
    "zcode",
    "pi",
    "qoder",
    "rovo-dev",
];

/// A brand agent logo (`getBrandAgentLogoStyle`): the SVG as a mask in the brand tint, except
/// OMP, whose artwork is multicolour and drawn as an image. `None` for an unknown icon id.
pub(crate) fn agent_logo(icon: &str, size: f32, hp: &HubPalette) -> Option<AnyElement> {
    let name = AGENT_ICONS.iter().find(|known| **known == icon)?;
    let path = SharedString::from(format!("agent-icons/{name}.svg"));
    if *name == "omp" {
        return Some(img(path).size(px(size)).flex_shrink_0().into_any_element());
    }
    Some(
        svg()
            .path(path)
            .size(px(size))
            .flex_shrink_0()
            .text_color(hsla(agent_logo_color(icon, hp)))
            .into_any_element(),
    )
}

/// `AgentLogo` (agents-hub-sync/shared.tsx (deleted 2026-10-01)): the brand logo, or the first letter of the name on
/// a quiet 5px-rounded tile when the agent has no icon.
pub(crate) fn sync_agent_logo(
    icon: Option<&str>,
    display_name: &str,
    size: f32,
    hp: &HubPalette,
) -> AnyElement {
    if let Some(logo) = icon.and_then(|icon| agent_logo(icon, size, hp)) {
        return logo;
    }
    let letter = display_name
        .chars()
        .next()
        .map(|ch| ch.to_uppercase().to_string())
        .unwrap_or_default();
    div()
        .flex_shrink_0()
        .size(px(size))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(if size >= 40.0 { 10.0 } else { 5.0 }))
        .border_1()
        .border_color(hsla(hp.line))
        .bg(hsla(hp.ink(0.12)))
        .text_color(hsla(hp.foreground))
        .text_size(px(if size >= 40.0 { 15.0 } else { 9.6 }))
        .font_weight(FontWeight::SEMIBOLD)
        .child(letter)
        .into_any_element()
}

pub(crate) fn tone_color(hp: &HubPalette, tone: SyncTone) -> Rgba {
    match tone {
        SyncTone::Ok => hp.ok,
        SyncTone::Warn => hp.warn,
        SyncTone::Err => hp.err,
        SyncTone::Off => hp.muted,
    }
}

/// `StatusDot`: 7px, filled in the tone, or an empty ring for a part that is not used.
pub(crate) fn status_dot(hp: &HubPalette, tone: SyncTone) -> AnyElement {
    let base = div().flex_shrink_0().size(px(7.0)).rounded_full();
    match tone {
        SyncTone::Off => base.border_1().border_color(hsla(hp.muted)),
        SyncTone::Ok => base.bg(hsla(hp.ok)),
        SyncTone::Warn => base.bg(hsla(hp.warn)),
        SyncTone::Err => base.bg(hsla(hp.err_dot)),
    }
    .into_any_element()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PillTone {
    Neutral,
    Ok,
    Warn,
    Err,
    Info,
}

/// `Pill`: an 11px/500 label in a 6px-rounded hairline chip, tinted by tone.
pub(crate) fn pill(hp: &HubPalette, tone: PillTone, label: impl Into<SharedString>) -> AnyElement {
    let (fill, border, text) = match tone {
        PillTone::Neutral => (transparent(), hsla(hp.line), hp.muted),
        PillTone::Ok => (hsla(hp.ok_fill()), hsla(hp.ok_border()), hp.ok),
        PillTone::Warn => (hsla(hp.warn_fill()), hsla(hp.warn_border()), hp.warn),
        PillTone::Err => (hsla(hp.err_fill()), hsla(hp.err_border()), hp.err),
        PillTone::Info => (hsla(hp.info_fill()), hsla(hp.info_border()), hp.info),
    };
    div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .px(px(7.2))
        .py(px(1.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(border)
        .bg(fill)
        .text_size(px(11.0))
        .line_height(px(16.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(hsla(text))
        .whitespace_nowrap()
        .child(label.into())
        .into_any_element()
}

/// `.agents-hub-sync-icon-tile`: a 30px, 7px-rounded tile around a 15px glyph, tinted by tone
/// (`None` is the plain raised tile).
pub(crate) fn icon_tile(hp: &HubPalette, tone: Option<SyncTone>, path: &'static str) -> AnyElement {
    let (fill, border, color) = match tone {
        Some(SyncTone::Ok) => (hp.ok_fill(), hp.ok_border(), hp.ok),
        Some(SyncTone::Warn) => (hp.warn_fill(), hp.warn_border(), hp.warn),
        Some(SyncTone::Err) => (hp.err_fill(), hp.err_border(), hp.err),
        _ => (hp.raised_hover, hp.line, hp.muted),
    };
    div()
        .flex_shrink_0()
        .size(px(30.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(7.0))
        .border_1()
        .border_color(hsla(border))
        .bg(hsla(fill))
        .child(icon(path, 15.0, color))
        .into_any_element()
}

/// `.agents-hub-empty`: a 13px muted line with 16px of room around it.
pub(crate) fn empty_note(hp: &HubPalette, text: impl Into<SharedString>) -> AnyElement {
    div()
        .flex()
        .items_center()
        .p(px(16.0))
        .text_size(px(13.0))
        .line_height(px(20.0))
        .text_color(hsla(hp.muted))
        .child(text.into())
        .into_any_element()
}

/// A text link (`.agents-hub-sync-link`): 12.5px muted, underlined at 40% of its colour.
pub(crate) fn text_link<V: 'static>(
    hp: &HubPalette,
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    use gpui::{InteractiveElement as _, StatefulInteractiveElement as _};
    let muted = hp.muted;
    let foreground = hp.foreground;
    div()
        .id(id)
        .flex_shrink_0()
        .text_size(px(12.48))
        .line_height(px(18.0))
        .text_color(hsla(muted))
        .text_decoration_1()
        .text_decoration_color(hsla(rgba_of(muted, 0.4)))
        .cursor_pointer()
        .hover(move |this| this.text_color(hsla(foreground)))
        .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
        .child(label.into())
}

/// Rounds a 1px hairline divider in the Hub's line colour.
pub(crate) fn hairline(hp: &HubPalette) -> Div {
    div().w_full().h(px(1.0)).flex_shrink_0().bg(hsla(hp.line))
}
