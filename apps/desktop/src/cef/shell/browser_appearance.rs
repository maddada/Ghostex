use super::*;

/// CDXC:Browser 2026-09-09 DECISION:
/// User: make the browser's white startup flashes full black.
/// Keep CEF's initial background and the renderer's default canvas override identical so appearance updates cannot reintroduce the flash.
pub(crate) const CEF_BROWSER_PAGE_BACKGROUND_COLOR: u32 = 0xFF00_0000;

/// CDXC:Browser 2026-09-08 DECISION:
/// User: replace the three new-tab/split options in the Browser overflow menu with System, Light, and Dark appearance detection, defaulting to System.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum BrowserPageAppearance {
    #[default]
    System,
    Light,
    Dark,
}

impl BrowserPageAppearance {
    pub(crate) const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    pub(crate) fn media_value(self) -> Option<&'static str> {
        match self {
            Self::Light => Some("light"),
            Self::Dark => Some("dark"),
            #[cfg(target_os = "macos")]
            Self::System => Some(if platform::system_uses_dark_page_appearance() {
                "dark"
            } else {
                "light"
            }),
            #[cfg(not(target_os = "macos"))]
            Self::System => None,
        }
    }
}

thread_local! {
    static BROWSER_PAGE_APPEARANCE: Cell<BrowserPageAppearance> = Cell::new(load_browser_page_appearance());
}

fn preference_path() -> PathBuf {
    crate::shared_settings::ghostex_storage_paths()
        .state_dir
        .join("gpui-browser-appearance.json")
}

fn load_browser_page_appearance() -> BrowserPageAppearance {
    let value = std::fs::read(preference_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
    match value
        .as_ref()
        .and_then(|value| value["appearance"].as_str())
    {
        Some("light") => BrowserPageAppearance::Light,
        Some("dark") => BrowserPageAppearance::Dark,
        _ => BrowserPageAppearance::System,
    }
}

pub(crate) fn browser_page_appearance() -> BrowserPageAppearance {
    BROWSER_PAGE_APPEARANCE.with(Cell::get)
}

pub(crate) fn set_browser_page_appearance(
    appearance: BrowserPageAppearance,
) -> std::io::Result<()> {
    let path = preference_path();
    std::fs::create_dir_all(path.parent().expect("browser appearance state directory"))?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let value = serde_json::json!({ "appearance": appearance.label().to_ascii_lowercase() });
    std::fs::write(&temporary, value.to_string())?;
    std::fs::rename(&temporary, &path)?;
    BROWSER_PAGE_APPEARANCE.with(|current| current.set(appearance));
    refresh_browser_page_appearances();
    Ok(())
}

pub(crate) fn refresh_browser_page_appearances() {
    refresh_chat_page_appearances();
    let browsers = SYSTEM_PAGE_APPEARANCE_CEF_NATIVE_VIEWS.with(|views| {
        CEF_BROWSERS_BY_NATIVE_VIEW.with(|browsers| {
            let browsers = browsers.borrow();
            views
                .borrow()
                .iter()
                .filter_map(|view| browsers.get(view).cloned())
                .collect::<Vec<_>>()
        })
    });
    for browser in browsers {
        apply_browser_page_appearance(&browser);
    }
}

thread_local! {
    /// CDXC:Theming 2026-09-12 WHY:
    /// The native app pins its own dark appearance, so chat must read the OS preference independently and keep following it even when Browser appearance is overridden.
    pub(crate) static CHAT_PAGE_APPEARANCE_CEF_NATIVE_VIEWS: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
}

pub(crate) fn system_page_color_scheme() -> Option<&'static str> {
    BrowserPageAppearance::System.media_value()
}

pub(crate) fn apply_page_color_scheme(browser: &cef::Browser, appearance: BrowserPageAppearance) {
    let Some(host) = browser.host() else {
        return;
    };
    /*
    CDXC:Browser 2026-09-08 WHY:
    macOS system detection must read the OS preference independently of NSApp's appearance, which can be pinned by the host.
    Apply the preference per Browser renderer because the Default profile shares its request context with app UI.
    On other platforms, an empty feature list restores Chromium's live system detection instead of overriding it with the old hardcoded light value.
    */
    let mut media_params = match cef::dictionary_value_create() {
        Some(params) => params,
        None => return,
    };
    media_params.set_string(Some(&CefString::from("media")), Some(&CefString::from("")));
    let mut features = match cef::list_value_create() {
        Some(features) => features,
        None => return,
    };
    if let Some(value) = appearance.media_value() {
        let Some(mut feature) = cef::dictionary_value_create() else {
            return;
        };
        feature.set_string(
            Some(&CefString::from("name")),
            Some(&CefString::from("prefers-color-scheme")),
        );
        feature.set_string(
            Some(&CefString::from("value")),
            Some(&CefString::from(value)),
        );
        features.set_dictionary(0, Some(&mut feature));
    }
    media_params.set_list(Some(&CefString::from("features")), Some(&mut features));
    host.execute_dev_tools_method(
        next_page_appearance_devtools_message_id(),
        Some(&CefString::from("Emulation.setEmulatedMedia")),
        Some(&mut media_params),
    );
}

pub(crate) fn refresh_chat_page_appearances() {
    let browsers = CHAT_PAGE_APPEARANCE_CEF_NATIVE_VIEWS.with(|views| {
        CEF_BROWSERS_BY_NATIVE_VIEW.with(|browsers| {
            let browsers = browsers.borrow();
            views
                .borrow()
                .iter()
                .filter_map(|view| browsers.get(view).cloned())
                .collect::<Vec<_>>()
        })
    });
    for browser in browsers {
        apply_page_color_scheme(&browser, BrowserPageAppearance::System);
    }
}
