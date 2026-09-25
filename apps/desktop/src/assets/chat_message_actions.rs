use serde::Deserialize;
use std::{collections::BTreeMap, sync::LazyLock};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessageActionIcons {
    stroke_width: f32,
    paths: BTreeMap<String, Vec<String>>,
}

static ICONS: LazyLock<MessageActionIcons> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../packages/gx-chat-core/visual/message-action-icons.json"
    ))
    .expect("shared message action artwork")
});

pub(crate) fn asset(key: &str) -> Option<String> {
    let paths = ICONS.paths.get(key)?;
    let body = paths
        .iter()
        .map(|path| format!(r#"<path d="{path}"/>"#))
        .collect::<String>();
    Some(format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round">{body}</svg>"#,
        ICONS.stroke_width
    ))
}
