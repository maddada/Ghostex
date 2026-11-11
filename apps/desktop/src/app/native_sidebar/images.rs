use crate::data_url_image::image;
use gpui::Image;
use std::sync::Arc;

pub(crate) fn sidebar_image(value: &str) -> Option<Arc<Image>> {
    image(value, false)
}

pub(crate) fn agent_image(value: &str, agent: Option<&str>, light: bool) -> Option<Arc<Image>> {
    image(
        value,
        light
            && matches!(
                agent,
                Some(
                    "codex"
                        | "copilot"
                        | "amp-cli"
                        | "cursor-cli"
                        | "grok-build"
                        | "mastra"
                        | "zcode"
                        | "freebuff"
                )
            ),
    )
}
