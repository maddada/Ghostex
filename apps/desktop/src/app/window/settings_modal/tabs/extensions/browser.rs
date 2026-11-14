//! `useExtensionsBrowserState` (packages/core-ui/extensions-modal/index.tsx (deleted 2026-10-01)): the installed list and
//! the catalog read from gxserver, the extension being shown on the detail page, the one waiting
//! for install consent, the operations in flight, the README and changelog of a Store entry, and
//! the extension icons (`extensionStaticAssetUrl`).
use super::super::super::store::{store_gxserver_rpc, store_http_get};
use super::ExtensionsTab;
use super::data::{
    CatalogEntry, InstalledExtension, catalog_asset_url, extension_static_asset_path,
    replace_installed, sort_installed,
};
use gpui::{Context, Image, ImageFormat};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

/// `fetch` has no time limit of its own; a download or an install can take a while.
const LIST_TIMEOUT: Duration = Duration::from_secs(60);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(600);
const STORE_CONTENT_TIMEOUT: Duration = Duration::from_secs(120);

/// `CatalogSnapshot`: the catalog and the URL its relative paths resolve against.
pub(crate) struct CatalogSnapshot {
    pub(crate) entries: Vec<CatalogEntry>,
    pub(crate) url: String,
}

/// An extension icon: loading, the SVG bytes to mask with the icon colour, or nothing to draw.
#[derive(Clone)]
pub(crate) enum IconSlot {
    Loading,
    Ready(Arc<[u8]>),
    Missing,
}

/// A Store screenshot: its decoded image and size.
#[derive(Clone)]
pub(crate) enum ScreenshotSlot {
    Loading,
    Ready(Arc<Image>, f32, f32),
    Missing,
}

#[derive(Default)]
pub(crate) struct BrowserState {
    pub(crate) installed: Vec<InstalledExtension>,
    pub(crate) catalog: Option<CatalogSnapshot>,
    pub(crate) selected_installed: Option<String>,
    pub(crate) selected_store: Option<String>,
    /// The catalog entry waiting for install consent.
    pub(crate) consent: Option<String>,
    /// The consent card's "install its agent skill too" box is ticked.
    pub(crate) consent_skill: bool,
    /// A folder's extension waiting for install consent (Install from folder…).
    pub(crate) folder_install: Option<super::folder_install::FolderInstall>,
    pub(crate) pending: HashSet<String>,
    pub(crate) loading: bool,
    pub(crate) error: Option<String>,
    pub(crate) readme: Option<String>,
    pub(crate) changelog: Option<String>,
    pub(crate) loading_content: bool,
    content_generation: u64,
    load_generation: u64,
    pub(crate) icons: HashMap<String, IconSlot>,
    pub(crate) screenshots: HashMap<String, ScreenshotSlot>,
}

impl BrowserState {
    pub(crate) fn catalog_entries(&self) -> &[CatalogEntry] {
        self.catalog
            .as_ref()
            .map(|catalog| catalog.entries.as_slice())
            .unwrap_or(&[])
    }

    pub(crate) fn catalog_entry(&self, name: &str) -> Option<&CatalogEntry> {
        self.catalog_entries()
            .iter()
            .find(|entry| entry.name() == name)
    }

    pub(crate) fn installed_extension(&self, id: &str) -> Option<&InstalledExtension> {
        self.installed.iter().find(|extension| extension.id() == id)
    }

    /// `selectedInstalled`.
    pub(crate) fn selected_installed(&self) -> Option<&InstalledExtension> {
        self.selected_installed
            .as_deref()
            .and_then(|id| self.installed_extension(id))
    }

    /// `selectedStore`.
    pub(crate) fn selected_store(&self) -> Option<&CatalogEntry> {
        self.selected_store
            .as_deref()
            .and_then(|name| self.catalog_entry(name))
    }

    /// `detailOpen`.
    pub(crate) fn detail_open(&self) -> bool {
        self.selected_installed().is_some() || self.selected_store().is_some()
    }

    pub(crate) fn consent_entry(&self) -> Option<&CatalogEntry> {
        self.consent
            .as_deref()
            .and_then(|name| self.catalog_entry(name))
    }
}

/// The page's notice when an install left the extension's skill out (a name clash, skills.rs).
pub(crate) fn skill_left_out_message(reason: String) -> String {
    format!("Skill not installed. {reason} Ghostex left that one alone.")
}

fn rpc_error(error: String) -> String {
    if error.trim().is_empty() {
        "The extension operation failed.".to_string()
    } else {
        error
    }
}

impl ExtensionsTab {
    /// Whether this host can reach gxserver (`createExtensionsModalTransport()` returned one).
    pub(crate) fn has_transport(&self, cx: &gpui::App) -> bool {
        self.settings_store_ref(cx).request().gxserver_rpc_available
    }

    /// `load()`: the installed registry and the catalog, together.
    pub(crate) fn load_browser(&mut self, cx: &mut Context<Self>) {
        if !self.has_transport(cx) {
            return;
        }
        self.browser.loading = true;
        self.browser.error = None;
        self.browser.load_generation += 1;
        let generation = self.browser.load_generation;
        let results: Rc<RefCell<(Option<Result<Value, String>>, Option<Result<Value, String>>)>> =
            Rc::new(RefCell::new((None, None)));
        let this = cx.weak_entity();
        let finish = {
            let results = results.clone();
            move |cx: &mut gpui::App| {
                let (list, catalog) = {
                    let results = results.borrow();
                    match (&results.0, &results.1) {
                        (Some(list), Some(catalog)) => (list.clone(), catalog.clone()),
                        _ => return,
                    }
                };
                let _ = this.update(cx, |page, cx| {
                    if page.browser.load_generation != generation {
                        return;
                    }
                    page.browser.loading = false;
                    match (list, catalog) {
                        (Ok(list), Ok(catalog)) => {
                            let mut installed: Vec<InstalledExtension> = list["extensions"]
                                .as_array()
                                .map(|items| {
                                    items
                                        .iter()
                                        .map(|raw| InstalledExtension { raw: raw.clone() })
                                        .collect()
                                })
                                .unwrap_or_default();
                            sort_installed(&mut installed);
                            page.browser.installed = installed;
                            page.browser.catalog = Some(CatalogSnapshot {
                                entries: catalog["catalog"]["extensions"]
                                    .as_array()
                                    .map(|items| {
                                        items
                                            .iter()
                                            .map(|raw| CatalogEntry { raw: raw.clone() })
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                                url: catalog["url"].as_str().unwrap_or_default().to_string(),
                            });
                        }
                        (Err(error), _) | (_, Err(error)) => {
                            page.browser.error = Some(if error.trim().is_empty() {
                                "Extensions could not be loaded.".to_string()
                            } else {
                                error
                            });
                        }
                    }
                    cx.notify();
                });
            }
        };
        let store = self.store.clone();
        let finish = Rc::new(finish);
        {
            let results = results.clone();
            let finish = finish.clone();
            store_gxserver_rpc(
                &store,
                "/api/listExtensions",
                json!({}),
                LIST_TIMEOUT,
                move |result, cx| {
                    results.borrow_mut().0 = Some(result);
                    finish(cx);
                },
                cx,
            );
        }
        store_gxserver_rpc(
            &store,
            "/api/extensionsCatalog",
            json!({}),
            LIST_TIMEOUT,
            move |result, cx| {
                results.borrow_mut().1 = Some(result);
                finish(cx);
            },
            cx,
        );
        cx.notify();
    }

    /// `runForExtension(id, operation)`: marks the extension busy while `path` runs, applies the
    /// result with `apply`, and shows a failure in the page's error banner.
    fn run_for_extension(
        &mut self,
        id: String,
        path: &'static str,
        params: Value,
        timeout: Duration,
        apply: impl FnOnce(&mut ExtensionsTab, Value) + 'static,
        cx: &mut Context<Self>,
    ) {
        self.browser.pending.insert(id.clone());
        self.browser.error = None;
        cx.notify();
        let this = cx.weak_entity();
        let store = self.store.clone();
        store_gxserver_rpc(
            &store,
            path,
            params,
            timeout,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    page.browser.pending.remove(&id);
                    match result {
                        Ok(value) => apply(page, value),
                        Err(error) => page.browser.error = Some(rpc_error(error)),
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// `setExtensionState(extension, patch)`.
    pub(crate) fn set_extension_state(&mut self, id: String, patch: Value, cx: &mut Context<Self>) {
        if !self.has_transport(cx) {
            return;
        }
        let params = json!({ "id": id, "patch": patch });
        self.run_for_extension(
            id,
            "/api/updateExtensionState",
            params,
            LIST_TIMEOUT,
            |page, value| {
                if value["extension"].is_object() {
                    replace_installed(
                        &mut page.browser.installed,
                        InstalledExtension {
                            raw: value["extension"].clone(),
                        },
                    );
                }
            },
            cx,
        );
    }

    /// `uninstallExtension(extension)`.
    pub(crate) fn uninstall_extension(&mut self, id: String, cx: &mut Context<Self>) {
        if !self.has_transport(cx) {
            return;
        }
        let removed = id.clone();
        self.run_for_extension(
            id.clone(),
            "/api/uninstallExtension",
            json!({ "id": id }),
            LIST_TIMEOUT,
            move |page, _| {
                page.browser
                    .installed
                    .retain(|extension| extension.id() != removed);
                if page.browser.selected_installed.as_deref() == Some(removed.as_str()) {
                    page.browser.selected_installed = None;
                }
            },
            cx,
        );
    }

    /// `installExtension(entry)`, with what the consent dialog showed as the approval, so gxserver
    /// grants only the permissions listed there and refuses a payload that asks for more.
    pub(crate) fn install_extension(&mut self, name: String, cx: &mut Context<Self>) {
        if !self.has_transport(cx) {
            return;
        }
        let skill = self.browser.consent_skill;
        let approval = self.browser.consent_entry().map(|entry| {
            json!({
                "version": entry.version(),
                "permissions": entry.permissions(),
                "backgroundProcess": entry.runs_background_process(),
                "remoteUrl": entry.remote_url(),
                "skill": skill && entry.skill_name().is_some(),
            })
        });
        self.run_for_extension(
            name.clone(),
            "/api/installExtension",
            json!({ "id": name, "approval": approval }),
            INSTALL_TIMEOUT,
            |page, value| {
                if value["extension"].is_object() {
                    let extension = InstalledExtension {
                        raw: value["extension"].clone(),
                    };
                    page.browser.icons.remove(&extension.id());
                    page.browser.error = extension.skill_left_out().map(skill_left_out_message);
                    replace_installed(&mut page.browser.installed, extension);
                }
                page.browser.consent = None;
            },
            cx,
        );
    }

    /// Opens a Store entry's page and reads its README and changelog.
    ///
    /// CDXC:Extensions 2026-11-14 WHY: gxserver reads both from the entry's hash-verified zip
    /// (`/api/extensionStoreContent`); the catalog's `readme`/`changelog` paths name files in the
    /// extensions repository that the store release never publishes, so fetching them from the
    /// catalog URL failed for every entry, installed or not.
    pub(crate) fn select_store_entry(&mut self, name: Option<String>, cx: &mut Context<Self>) {
        self.browser.selected_store = name;
        self.browser.content_generation += 1;
        let generation = self.browser.content_generation;
        let Some(id) = self.browser.selected_store().map(|entry| entry.name()) else {
            self.browser.readme = None;
            self.browser.changelog = None;
            self.browser.loading_content = false;
            cx.notify();
            return;
        };
        self.browser.loading_content = true;
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/extensionStoreContent",
            json!({ "id": id }),
            STORE_CONTENT_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    if page.browser.content_generation != generation {
                        return;
                    }
                    page.browser.loading_content = false;
                    let text = |value: &Value, key: &str| value[key].as_str().map(str::to_string);
                    match result {
                        Ok(value) => {
                            page.browser.readme = text(&value, "readme");
                            page.browser.changelog = text(&value, "changelog");
                        }
                        Err(_) => {
                            page.browser.readme = None;
                            page.browser.changelog = None;
                        }
                    }
                    cx.notify();
                });
            },
            cx,
        );
        cx.notify();
    }

    /// `iconUrlForInstalled` / `iconUrlForCatalogEntry`: the icon of an installed extension, read
    /// once from gxserver's static files. `None` while it loads or when there is nothing to draw.
    pub(crate) fn extension_icon(
        &mut self,
        id: &str,
        icon: &str,
        cx: &mut Context<Self>,
    ) -> Option<Arc<[u8]>> {
        if !self.has_transport(cx) || icon.is_empty() {
            return None;
        }
        match self.browser.icons.get(id) {
            Some(IconSlot::Ready(bytes)) => return Some(bytes.clone()),
            Some(IconSlot::Loading | IconSlot::Missing) => return None,
            None => {}
        }
        self.browser.icons.insert(id.to_string(), IconSlot::Loading);
        let key = id.to_string();
        let this = cx.weak_entity();
        store_http_get(
            &self.store.clone(),
            extension_static_asset_path(id, icon),
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    let slot = match result {
                        Ok(bytes) if !bytes.is_empty() && looks_like_svg(&bytes) => {
                            IconSlot::Ready(Arc::from(bytes.into_boxed_slice()))
                        }
                        _ => IconSlot::Missing,
                    };
                    page.browser.icons.insert(key, slot);
                    cx.notify();
                });
            },
            cx,
        );
        None
    }

    /// The icon of a Store entry that is not installed, read once from its zip through gxserver
    /// (`/api/extensionStoreContent`): it has no static files to serve the icon from yet.
    /// `None` while it loads or when there is nothing to draw.
    pub(crate) fn store_icon(
        &mut self,
        id: &str,
        icon: &str,
        cx: &mut Context<Self>,
    ) -> Option<Arc<[u8]>> {
        if !self.has_transport(cx) || icon.is_empty() {
            return None;
        }
        let key = format!("store:{id}");
        match self.browser.icons.get(&key) {
            Some(IconSlot::Ready(bytes)) => return Some(bytes.clone()),
            Some(IconSlot::Loading | IconSlot::Missing) => return None,
            None => {}
        }
        self.browser.icons.insert(key.clone(), IconSlot::Loading);
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/extensionStoreContent",
            json!({ "id": id }),
            STORE_CONTENT_TIMEOUT,
            move |result, cx| {
                let slot = match result
                    .ok()
                    .and_then(|value| value["icon"].as_str().map(|icon| icon.as_bytes().to_vec()))
                {
                    Some(bytes) if !bytes.is_empty() && looks_like_svg(&bytes) => {
                        IconSlot::Ready(Arc::from(bytes.into_boxed_slice()))
                    }
                    _ => IconSlot::Missing,
                };
                let _ = this.update(cx, |page, cx| {
                    page.browser.icons.insert(key, slot);
                    cx.notify();
                });
            },
            cx,
        );
        None
    }

    /// Whether [`Self::store_icon`] is still reading this entry's icon.
    pub(crate) fn store_icon_loading(&self, id: &str) -> bool {
        matches!(
            self.browser.icons.get(&format!("store:{id}")),
            Some(IconSlot::Loading)
        )
    }

    /// A Store screenshot (`<img src>` from the catalog), decoded once.
    pub(crate) fn screenshot(&mut self, url: &str, cx: &mut Context<Self>) -> ScreenshotSlot {
        if let Some(slot) = self.browser.screenshots.get(url) {
            return slot.clone();
        }
        self.browser
            .screenshots
            .insert(url.to_string(), ScreenshotSlot::Loading);
        let key = url.to_string();
        let this = cx.weak_entity();
        store_http_get(
            &self.store.clone(),
            url.to_string(),
            move |result, cx| {
                let slot = result
                    .ok()
                    .and_then(|bytes| {
                        let format = image::guess_format(&bytes).ok()?;
                        let (width, height) =
                            image::ImageReader::with_format(std::io::Cursor::new(&bytes), format)
                                .into_dimensions()
                                .ok()?;
                        let gpui_format = match format {
                            image::ImageFormat::Png => ImageFormat::Png,
                            image::ImageFormat::Jpeg => ImageFormat::Jpeg,
                            image::ImageFormat::Gif => ImageFormat::Gif,
                            image::ImageFormat::WebP => ImageFormat::Webp,
                            _ => return None,
                        };
                        Some(ScreenshotSlot::Ready(
                            Arc::new(Image::from_bytes(gpui_format, bytes)),
                            width as f32,
                            height as f32,
                        ))
                    })
                    .unwrap_or(ScreenshotSlot::Missing);
                let _ = this.update(cx, |page, cx| {
                    page.browser.screenshots.insert(key, slot);
                    cx.notify();
                });
            },
            cx,
        );
        ScreenshotSlot::Loading
    }
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_lowercase();
    head.contains("<svg") || head.starts_with("<?xml")
}

/// `new URL(path, catalogUrl)` for a Store entry's screenshots.
pub(crate) fn screenshot_urls(browser: &BrowserState, entry: &CatalogEntry) -> Vec<String> {
    let Some(catalog) = browser.catalog.as_ref() else {
        return Vec::new();
    };
    entry
        .screenshots()
        .iter()
        .filter_map(|path| catalog_asset_url(&catalog.url, path))
        .collect()
}
