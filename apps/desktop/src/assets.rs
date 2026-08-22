use anyhow::anyhow;
use gpui::{AssetSource, Result, SharedString};
use rust_embed::RustEmbed;
use std::borrow::Cow;

#[derive(RustEmbed)]
#[folder = "assets"]
#[include = "titlebar/**/*.svg"]
struct GhostexEmbeddedAssets;

#[derive(RustEmbed)]
#[folder = "../../src/assets"]
#[include = "*.svg"]
struct GhostexAgentIconAssets;

pub(crate) struct GhostexAssets;

impl AssetSource for GhostexAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }
        if path.starts_with("titlebar/") {
            return GhostexEmbeddedAssets::get(path)
                .map(|asset| Some(asset.data))
                .ok_or_else(|| anyhow!("could not find embedded Ghostex asset at path {path:?}"));
        }
        if let Some(agent_icon_path) = path.strip_prefix("agent-icons/") {
            return GhostexAgentIconAssets::get(agent_icon_path)
                .map(|asset| Some(asset.data))
                .ok_or_else(|| anyhow!("could not find embedded agent icon at path {path:?}"));
        }
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut assets = GhostexEmbeddedAssets::iter()
            .filter_map(|asset| asset.starts_with(path).then(|| asset.into()))
            .collect::<Vec<_>>();
        assets.extend(GhostexAgentIconAssets::iter().filter_map(|asset| {
            let asset = format!("agent-icons/{asset}");
            asset.starts_with(path).then(|| asset.into())
        }));
        assets.extend(gpui_component_assets::Assets.list(path)?);
        assets.sort_unstable();
        assets.dedup();
        Ok(assets)
    }
}
