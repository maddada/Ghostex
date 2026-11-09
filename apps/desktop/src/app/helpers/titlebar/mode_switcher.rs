use crate::*;

pub(crate) fn titlebar_mode_switcher_items(
    availability: ProjectScopedWorkareaAvailability,
) -> Vec<TitlebarModeSwitcherItem> {
    /*
    CDXC:Titlebar 2026-07-04-01:00:
    The GPUI titlebar mode list mirrors macOS Quick/projectless presentation: Agents and Source are always visible and selectable; Browser, Kanban, Automate, and Docs stay visible but disabled in Quick context. Activation, hotkeys, restored active mode, and persisted active mode delegate to the same context availability helper.

    CDXC:Workarea 2026-06-22-18:00:
    Kanban, Automate, and Docs must be unavailable without a project, and GPUI currently shares Browser's Quick/projectless disablement through the same titlebar contract. Until a real GPUI project/sidebar snapshot exists, keep GHOSTEX_GPUI_PROJECT_IS_QUICK isolated behind GpuiProjectContext and pass a typed ProjectScopedWorkareaAvailability into mode lists and action guards instead of adding git/path heuristics or fallback project detection.

    CDXC:Workarea 2026-06-22-19:44:
    Runtime App titlebar mode lists, activation guards, and active-mode coercion prefer the latest valid in-memory sidebar project snapshot when available. The fallback availability is supplied by the caller so app runtime code can choose its current strict source without persisting or logging raw snapshot details.

    CDXC:CefRuntime 2026-07-04-01:00:
    App-owned titlebar mode lists and active-mode fallback receive fallback availability from the current project context, but Docs/Manage titlebar visibility is unconditional and only project context can disable it.
    */
    availability.titlebar_mode_switcher_items()
}

pub(crate) struct GpuiExtensionViewPresentation {
    pub(crate) title: String,
    pub(crate) server_is_static: bool,
}

#[derive(Clone)]
pub(crate) struct GpuiCustomView {
    pub(crate) enabled: bool,
    pub(crate) id: ExtensionId,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) definition: serde_json::Value,
}

pub(crate) fn gpui_custom_views_from_settings() -> Vec<GpuiCustomView> {
    std::iter::once(crate::app::storybook::storybook_view())
        .chain(crate::app::project_websites::website_views())
        .chain(std::iter::once(crate::app::claude_web::claude_web_view()))
        .chain(std::iter::once(crate::app::claude_web::grok_web_view()))
        .chain(
            shared_settings::shared_sidebar_settings_snapshot()
                .object()
                .get("customViews")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|value| {
                    let object = value.as_object()?;
                    let id = object.get("id")?.as_str()?.trim();
                    if !id.starts_with("custom-view-") {
                        return None;
                    }
                    let id = ExtensionId::new(id)?;
                    let title = object.get("name")?.as_str()?.trim();
                    let url = object
                        .get("url")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("")
                        .trim();
                    if title.is_empty() {
                        return None;
                    }
                    if object.get("source").is_none() {
                        let (scheme, rest) = url.split_once("://")?;
                        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
                        if !matches!(scheme, "http" | "https")
                            || authority.is_empty()
                            || url.chars().any(char::is_whitespace)
                        {
                            return None;
                        }
                    }
                    Some(GpuiCustomView {
                        enabled: object
                            .get("enabled")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(true),
                        id,
                        title: title.to_string(),
                        url: url.to_string(),
                        definition: value.clone(),
                    })
                }),
        )
        .collect()
}

pub(crate) fn gpui_custom_view(id: ExtensionId) -> Option<GpuiCustomView> {
    gpui_custom_views_from_settings()
        .into_iter()
        .find(|view| view.id == id)
}

pub(crate) fn gpui_enabled_custom_view(id: ExtensionId) -> Option<GpuiCustomView> {
    gpui_custom_view(id).filter(|view| view.enabled)
}

pub(crate) fn gpui_extension_view_presentation(
    id: ExtensionId,
) -> Option<GpuiExtensionViewPresentation> {
    if let Some(view) = gpui_custom_view(id) {
        if !view.enabled {
            return None;
        }
        return Some(GpuiExtensionViewPresentation {
            title: view.title,
            server_is_static: false,
        });
    }
    let payload_dir = shared_settings::ghostex_storage_paths()
        .extensions_dir()
        .join("installed")
        .join(id.as_str());
    let manifest_text = std::fs::read_to_string(payload_dir.join("ghostex-extension.json")).ok()?;
    let manifest = serde_json::from_str::<serde_json::Value>(&manifest_text)
        .ok()?
        .as_object()?
        .clone();
    if manifest.get("name")?.as_str()? != id.as_str() {
        return None;
    }
    let title = manifest.get("title")?.as_str()?.trim().to_string();
    if title.is_empty() {
        return None;
    }
    Some(GpuiExtensionViewPresentation {
        title,
        server_is_static: manifest
            .get("server")
            .and_then(serde_json::Value::as_object)
            .is_some_and(|server| server.get("static").is_some()),
    })
}

pub(crate) fn titlebar_mode_view_tab_hidden_settings_key(
    mode: TitlebarMode,
) -> Option<&'static str> {
    match mode {
        mode if mode.website_provider().is_some() => mode
            .website_provider()
            .map(|provider| provider.hidden_settings_key.as_str()),
        mode if mode.is_storybook() => Some("storybookViewTabHidden"),
        TitlebarMode::Source => Some(SOURCE_CODE_VIEW_TAB_HIDDEN_SETTINGS_KEY),
        TitlebarMode::Browser => Some(BROWSER_VIEW_TAB_HIDDEN_SETTINGS_KEY),
        TitlebarMode::Kanban => Some(KANBAN_VIEW_TAB_HIDDEN_SETTINGS_KEY),
        TitlebarMode::Automate => Some(AUTOMATE_VIEW_TAB_HIDDEN_SETTINGS_KEY),
        TitlebarMode::Manage => Some(DOCS_VIEW_TAB_HIDDEN_SETTINGS_KEY),
        TitlebarMode::Changes => Some(CHANGES_VIEW_TAB_HIDDEN_SETTINGS_KEY),
        TitlebarMode::Terminal => Some(TERMINAL_VIEW_TAB_HIDDEN_SETTINGS_KEY),
        TitlebarMode::BotFeed => Some(BOT_AUTOMATIONS_HIDDEN_SETTINGS_KEY),
        TitlebarMode::Agents
        | TitlebarMode::Work
        | TitlebarMode::SideChat
        | TitlebarMode::Extension(_) => None,
    }
}

/// The Official switch a view needs on as well as its own, the Rust side of a descriptor's
/// `requiresExtension` (packages/shared/ghostex-official-extensions.ts (deleted 2026-10-01)).
fn titlebar_mode_required_hidden_settings_key(mode: TitlebarMode) -> Option<&'static str> {
    (mode == TitlebarMode::BotFeed).then_some(BOTS_HIDDEN_SETTINGS_KEY)
}

/// A view whose switch is off until the user turns it on, so a settings file without the key hides it.
fn titlebar_mode_hidden_by_default(mode: TitlebarMode) -> bool {
    mode == TitlebarMode::BotFeed
        || mode
            .website_provider()
            .is_some_and(|provider| provider.hidden_by_default)
}

pub(crate) fn gpui_titlebar_mode_hidden_from_settings(mode: TitlebarMode) -> bool {
    let Some(settings_key) = titlebar_mode_view_tab_hidden_settings_key(mode) else {
        return false;
    };
    let settings = shared_settings::shared_sidebar_settings_snapshot();
    let hidden = |key: &str| {
        settings
            .object()
            .get(key)
            .and_then(serde_json::Value::as_bool)
    };
    // The one required switch (Bots) is hidden by default too, so only an explicit `false` counts.
    let required_off = titlebar_mode_required_hidden_settings_key(mode)
        .is_some_and(|key| hidden(key) != Some(false));
    required_off || hidden(settings_key).unwrap_or_else(|| titlebar_mode_hidden_by_default(mode))
}
