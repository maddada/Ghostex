use anyhow::anyhow;
use gpui::{AssetSource, Result, SharedString};
use rust_embed::RustEmbed;
use std::borrow::Cow;

#[path = "assets/chat_message_actions.rs"]
mod chat_message_actions;
#[path = "assets/chat_references.rs"]
pub(crate) mod chat_references;
#[path = "assets/chat_working.rs"]
pub(crate) mod chat_working;
#[path = "assets/onboarding.rs"]
mod onboarding;

#[derive(RustEmbed)]
#[folder = "assets"]
#[include = "titlebar/**/*.svg"]
#[include = "modals/**/*.svg"]
// CDXC:Docs 2026-09-28 WHY: the Files view's icons live in files-view/ (they were in docs/), because the root .gitignore ignores every folder named docs/ and the old assets/docs/ icons never reached git (a fresh checkout drew Files without icons).
#[include = "files-view/**/*.svg"]
#[include = "capture/**/*.svg"]
struct GhostexEmbeddedAssets;

/// The 256px app icon, embedded once for every reader: Settings > About, the Linux window icon and
/// the Ghostex Capture button (which used the byte-identical `icon_128x128@2x.png`).
pub(crate) static APP_ICON_PNG: &[u8] =
    include_bytes!("../resources/AppIcon.appiconset/icon_256x256.png");

/// DM Sans's static 400, 500, 600 and 700 faces, embedded once for the chat (`native_chat/fonts.rs`)
/// and the onboarding (`window/onboarding/fonts.rs`), which both register them. They live here
/// because every build that compiles either file (the app, apps/gpui-web and the modal demo) has
/// this module.
pub(crate) static DM_SANS_UPRIGHT_FACES: [&[u8]; 4] = [
    include_bytes!("../../../.dependencies/app-fonts/dm-sans/static/400.ttf"),
    include_bytes!("../../../.dependencies/app-fonts/dm-sans/static/500.ttf"),
    include_bytes!("../../../.dependencies/app-fonts/dm-sans/static/600.ttf"),
    include_bytes!("../../../.dependencies/app-fonts/dm-sans/static/700.ttf"),
];

pub(crate) struct GhostexAssets;

impl AssetSource for GhostexAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }
        if let Some(key) = path.strip_prefix("chat-references/") {
            return chat_references::asset(key)
                .map(|svg| Some(Cow::Borrowed(svg)))
                .ok_or_else(|| anyhow!("unknown chat reference asset {key:?}"));
        }
        if let Some(key) = path.strip_prefix("chat-actions/") {
            return chat_message_actions::asset(key)
                .map(|svg| Some(Cow::Owned(svg.into_bytes())))
                .ok_or_else(|| anyhow!("unknown message action asset {key:?}"));
        }
        if let Some(key) = path.strip_prefix("chat-working/") {
            return chat_working::asset(key)
                .map(|svg| Some(Cow::Owned(svg.into_bytes())))
                .ok_or_else(|| anyhow!("unknown working strip asset {key:?}"));
        }
        // CDXC:Settings 2026-10-07 DECISION: The user said "yes show real icon": Settings > About draws the real Ghostex app icon (the same PNG the window and Dock use), not a "G" placeholder tile.
        if path == "app-icon.png" {
            return Ok(Some(Cow::Borrowed(APP_ICON_PNG)));
        }
        if let Some(key) = path.strip_prefix("onboarding/") {
            return onboarding::asset(key)
                .map(Some)
                .ok_or_else(|| anyhow!("unknown onboarding asset {key:?}"));
        }
        if path.starts_with("titlebar/")
            || path.starts_with("modals/")
            || path.starts_with("files-view/")
            || path.starts_with("capture/")
        {
            return GhostexEmbeddedAssets::get(path)
                .map(|asset| Some(asset.data))
                .ok_or_else(|| anyhow!("could not find embedded Ghostex asset at path {path:?}"));
        }
        // The MCP app logos a tool-call card draws (gx-chat-core `transcript/tool_apps.rs`).
        if let Some(key) = path.strip_prefix("chat-tool-apps/") {
            return key
                .strip_suffix(".svg")
                .and_then(ghostex_gx_chat_core::transcript::tool_apps::tool_app_svg)
                .map(|svg| Some(Cow::Borrowed(svg.as_bytes())))
                .ok_or_else(|| anyhow!("unknown tool app logo {key:?}"));
        }
        // CDXC:Icons 2026-10-22 WHY: `agent-icons/<key>.svg` is served from gx-core's agent logo table, which already embeds every `packages/core-ui/assets/*.svg` for the sidebar's coloured logos; a second RustEmbed of the same folder put the artwork in the binary twice. A new agent logo goes into that table (`packages/gx-core/src/sidebar_menu/agent_logos.rs`).
        if let Some(agent_icon_path) = path.strip_prefix("agent-icons/") {
            return agent_icon_path
                .strip_suffix(".svg")
                .and_then(ghostex_gx_core::agent_logo_svg)
                .map(|svg| Some(Cow::Borrowed(svg.as_bytes())))
                .ok_or_else(|| anyhow!("could not find embedded agent icon at path {path:?}"));
        }
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut assets = GhostexEmbeddedAssets::iter()
            .filter_map(|asset| asset.starts_with(path).then(|| asset.into()))
            .collect::<Vec<_>>();
        assets.extend(
            ghostex_gx_core::agent_logo_icons()
                .into_iter()
                .filter_map(|icon| {
                    let asset = format!("agent-icons/{icon}.svg");
                    asset.starts_with(path).then(|| asset.into())
                }),
        );
        assets.extend(gpui_component_assets::Assets.list(path)?);
        assets.sort_unstable();
        assets.dedup();
        Ok(assets)
    }
}
