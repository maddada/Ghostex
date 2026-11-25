//! The saved links (`customLinks` in the settings file, this computer's only) and which of them the
//! link page shows, remembered across restarts in `custom-link-page.json` beside Grok's
//! `grok-web-login.json`.

use std::cell::RefCell;

use base64::Engine as _;
use ghostex_gx_core::custom_links::{
    CustomLink, custom_link_menu_rows, custom_links_from_settings,
};
use serde_json::{Value, json};

use crate::app::helpers::*;
use crate::*;

const PAGE_FILE: &str = "custom-link-page.json";

/// The one disk-backed browser profile every link's page opens on (its own cookies, kept across
/// restarts, apart from the projects' Browser and Claude's and Grok's sign-ins:
/// cef/shell/workspace_browser.rs).
pub(crate) const CUSTOM_LINK_BROWSER_PROFILE: &str = "workspace-custom-links";

/// What `custom-link-page.json` keeps: the link the page shows and the Browser profile it opens in
/// (`None`: the links' own profile, `CUSTOM_LINK_BROWSER_PROFILE`).
#[derive(Clone, Debug, Default)]
struct PageFile {
    link_id: Option<String>,
    profile: Option<u64>,
}

thread_local! {
    /// The page file, read the first time it is asked.
    static PICKED: RefCell<Option<PageFile>> = const { RefCell::new(None) };
}

/// The links the ⋯ menu lists, in their saved order.
pub(crate) fn saved_custom_links() -> Vec<CustomLink> {
    custom_links_from_settings(shared_settings::shared_sidebar_settings_snapshot().object())
}

pub(crate) fn saved_custom_link(id: &str) -> Option<CustomLink> {
    saved_custom_links().into_iter().find(|link| link.id == id)
}

fn page_file() -> std::path::PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .state_dir
        .join(PAGE_FILE)
}

fn with_page_file<T>(f: impl FnOnce(&mut PageFile) -> T) -> T {
    PICKED.with(|picked| {
        let mut picked = picked.borrow_mut();
        let file = picked.get_or_insert_with(|| {
            let saved = std::fs::read_to_string(page_file())
                .ok()
                .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                .unwrap_or_default();
            PageFile {
                link_id: saved["linkId"].as_str().map(str::to_string),
                profile: saved["profileId"].as_u64(),
            }
        });
        f(file)
    })
}

fn save_page_file() {
    let (link_id, profile) = with_page_file(|file| (file.link_id.clone(), file.profile));
    let path = page_file();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(
        path,
        json!({ "linkId": link_id, "profileId": profile }).to_string(),
    );
}

fn picked_link_id() -> Option<String> {
    with_page_file(|file| file.link_id.clone())
}

/// The id of the link the page shows, saved or not; part of the custom-view list's cache key
/// (helpers/titlebar/mode_switcher.rs).
pub(crate) fn custom_link_page_link_id() -> Option<String> {
    picked_link_id()
}

pub(crate) fn set_custom_link_page_link(id: Option<&str>) {
    with_page_file(|file| file.link_id = id.map(str::to_string));
    save_page_file();
}

/// The Browser profile the page opens in, picked from the toolbar's Profile button; `None` is the
/// links' own profile.
pub(crate) fn custom_link_page_profile() -> Option<BrowserProfileId> {
    with_page_file(|file| file.profile).map(BrowserProfileId)
}

pub(crate) fn set_custom_link_page_profile(profile: Option<BrowserProfileId>) {
    with_page_file(|file| file.profile = profile.map(|profile| profile.0));
    save_page_file();
}

/// The link the page shows: the one opened last, while it is still saved.
pub(crate) fn custom_link_page_link() -> Option<CustomLink> {
    saved_custom_link(&picked_link_id()?)
}

/// The icon the Browser cached for the link's site (helpers/browser_favicon_cache.rs), as a
/// `data:` image for the menu row; the row shows the globe without one.
pub(crate) fn custom_link_favicon_data_url(link: &CustomLink) -> Option<String> {
    let favicon = browser_favicon_cache_lookup(&link.url, None)?;
    let image = &favicon.image;
    Some(format!(
        "data:{};base64,{}",
        image.format().mime_type(),
        base64::engine::general_purpose::STANDARD.encode(image.bytes())
    ))
}

/// The ⋯ menu's link rows and Add link…, as the menu opens.
pub(crate) fn custom_link_menu_rows_now() -> Vec<Value> {
    custom_link_menu_rows(&saved_custom_links(), custom_link_favicon_data_url)
}
