//! `getBrandAgentLogoStyle` (packages/core-ui/agent-logos.ts (deleted 2026-10-01)): an agent's logo as a 16px mask in
//! its brand tint (`AGENT_LOGO_COLORS`), except OMP, whose artwork is multicolour and drawn as an
//! image. The white and near-white logos take the foreground in light themes, Codex follows
//! `--ghostex-codex-logo`, and Z.ai is black on light UI (CDXC:Icons 2026-09-15 DECISION in
//! agent-logos.ts (deleted 2026-10-01)). The icon tile around it is `.settings-management-icon`.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::settings_icon;
use super::super::super::palette::SettingsPalette;
use super::icons;
use gpui::{
    AnyElement, IntoElement, ParentElement as _, SharedString, Styled as _, div, img, px, rgb, svg,
};

/// The agent icons shipped under `agent-icons/` (`SidebarAgentIcon`).
const AGENT_ICONS: [&str; 28] = [
    "amp-cli",
    "antigravity-cli",
    "browser",
    "claude",
    "claude-cloud",
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

/// `AGENT_LOGO_COLORS`.
fn brand_color(icon: &str) -> u32 {
    match icon {
        "antigravity-cli" => 0x749bff,
        "browser" => 0x82b7ff,
        "claude" => 0xd97757,
        "claude-cloud" => 0xd67556,
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
    }
}

fn logo_color(icon: &str, p: &SettingsPalette) -> gpui::Rgba {
    let brand = brand_color(icon);
    if p.light && matches!(brand, 0xffffff | 0xedecec) {
        return if icon == "zcode" {
            rgb(0x000000)
        } else {
            p.foreground
        };
    }
    rgb(brand)
}

/// The brand logo of `icon` at `size`, or `None` for an icon without a logo.
pub(super) fn agent_logo(icon: &str, size: f32, p: &SettingsPalette) -> Option<AnyElement> {
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
            .text_color(hsla(logo_color(icon, p)))
            .into_any_element(),
    )
}

/// `SettingsAgentIcon`: the brand logo, or the code-dots glyph for an agent without an icon.
pub(super) fn agent_icon(icon: Option<&str>, p: &SettingsPalette) -> AnyElement {
    icon.and_then(|icon| agent_logo(icon, 16.0, p))
        .unwrap_or_else(|| {
            settings_icon(icons::CODE_DOTS, 16.0, p.foreground)
                .flex_shrink_0()
                .into_any_element()
        })
}

/// The muted tile colour (`bg-muted`): the icon tile and an open disclosure button.
pub(super) fn muted_fill(p: &SettingsPalette) -> gpui::Rgba {
    if p.light {
        rgb(0xefefef)
    } else {
        rgb(0x2a2a2a)
    }
}

/// `.settings-management-icon`: the 36px tile with the agent's icon.
pub(super) fn agent_icon_tile(icon: Option<&str>, p: &SettingsPalette) -> AnyElement {
    div()
        .flex_shrink_0()
        .size(px(36.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .bg(hsla(muted_fill(p)))
        .child(agent_icon(icon, p))
        .into_any_element()
}
