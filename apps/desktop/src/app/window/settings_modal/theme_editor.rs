//! Compact draggable floating Theme Editor window (Q4386: Floating).
//!
//! Opened from the top of Settings > Themes (closing Settings so the user sees the main Ghostex
//! window live behind it) or after onboarding's "Make it yours" step. Single scrollable column
//! (~320px wide) aligned with Settings > Themes: Appearance, Theme tiles, the single Artwork strip
//! (None, all artworks, Your image, Animated at the end) with its controls directly below, Dark /
//! Light colour squares + uncapped Custom colour, Colourfulness & separate sidebar/work-area
//! contrast, Accent & Fine-tune colour roles, the slim Window transparency section (paused while an
//! artwork is active, with Fully transparent / Wallpaper tiles), Chat & Terminal appearance, Undo,
//! Reset, and "All theme settings" back to Settings.
use super::super::native_modal_kit::*;
use super::catalog::{module, settings_catalog};
use super::fields::{
    ColorSaver, FieldStates, SettingsPage, SliderBinding, SliderSaver,
    centered_tint_picker_overlay, open_tint_picker, set_color_saver, settings_icon, step_slider,
    tooltip_text,
};
use super::model::SettingsModalHost;
use super::palette::{SETTINGS_FONT, SettingsPalette};
use super::restore::OpenFormMemory;
use super::shell::SettingsModalConfig;
use super::store::{
    SettingsStore, SettingsValues, new_settings_store, post_store_message, store_gxserver_rpc,
};
use super::tabs::theme::art::source_art;
use super::tabs::theme::artwork::{
    Backdrop, art_key, backdrop, file_preview, live_shows, painted_name, your_image_path,
};
use super::tabs::theme::colours::{
    COLOURFULNESS_LAST_POSITION, STRENGTH_MAX, STRENGTH_MIN, STRENGTH_STEP,
    area_colourfulness_patch, colourfulness_display_step, colourfulness_name, colourfulness_patch,
    colourfulness_points, colourfulness_step_index, paired_preset, preset_label, preset_options,
    preview_scheme_dark, strength_from_settings, strength_gaps, strength_patch, swatch_paint,
};
use super::tabs::theme::gallery::poster as live_poster;
use super::tabs::theme::presets::{theme_preset_matches, theme_presets};
use super::tabs::theme::prompt_gallery::{
    GalleryTheme, applied_theme, artwork_thumbnail_bytes, gallery_themes, preview_bytes,
};
use super::tabs::theme::role_defaults::role_default;
use super::tabs::theme::roles::{alpha_of, hex_of, saved_role, with_role};
use super::tabs::theme::swatches::swatch_colour;
use super::tabs::theme::transparency::{
    SHOW_BEHIND, same_for_light_and_dark, transparency_choices,
};
use ghostex_settings_catalog::theme_roles::{
    RoleValue, THEME_ROLES_SETTING, format_role_value, parse_role_value,
};
use ghostex_settings_catalog::{ARTWORKS, YOUR_IMAGE_ART_ID, artwork_by_id};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, FocusHandle, Focusable, Image, ImageFormat,
    InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton, ObjectFit, ParentElement as _,
    Pixels, Point, Render, RenderImage, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, StyledImage as _, Subscription, Window,
    WindowAppearance, WindowControlArea, canvas, div, img, point, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Map, Value, json};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

/// Width and height of the compact floating Theme Editor window.
pub(crate) const THEME_EDITOR_WIDTH: f32 = 320.0;
pub(crate) const THEME_EDITOR_HEIGHT: f32 = 640.0;

const HEADER_HEIGHT: f32 = 40.0;
const FOOTER_HEIGHT: f32 = 38.0;
const INITIAL_THEME_TILES: usize = 8;
const DRAG_THRESHOLD: f32 = 3.0;

const ICON_GRIP: &str = "modals/settings/grip-vertical.svg";
const ICON_UNDO: &str = "modals/settings/arrow-back-up.svg";
const ICON_CLOSE: &str = "modals/settings/x.svg";
const ICON_SETTINGS: &str = "modals/settings/settings.svg";
const ICON_CHEVRON_RIGHT: &str = "modals/settings/chevron-right.svg";
const ICON_CHEVRON_DOWN: &str = "modals/settings/chevron-down.svg";
const ICON_PLUS: &str = "modals/settings/plus.svg";
const ICON_RESET: &str = "modals/settings/refresh.svg";

const ACCENT_DOTS: [&str; 8] = [
    "#86d3f8", "#7cb8f7", "#d8664f", "#7ecdc5", "#a7c080", "#d8a657", "#d69bb5", "#bd93f9",
];

const FINE_TUNE_ROLES: [(&str, &str); 12] = [
    ("userMessage", "Your messages"),
    ("chatBox", "Chat box"),
    ("surface", "Cards"),
    ("control", "Controls"),
    ("menu", "Menus"),
    ("text", "Text"),
    ("mutedText", "Grey text"),
    ("link", "Links"),
    ("border", "Borders"),
    ("success", "Success"),
    ("warning", "Warning"),
    ("danger", "Danger"),
];

const THEME_ENDPOINT: &str = "/api/theme";
const THEME_TIMEOUT: Duration = Duration::from_secs(20);

fn custom_tint_key(dark: bool) -> &'static str {
    if dark {
        "customSidebarTitlebarBackgroundTintColor"
    } else {
        "customSidebarTitlebarLightBackgroundTintColor"
    }
}

fn role_picker_key(role: &str, light: bool) -> &'static str {
    use std::cell::RefCell;
    thread_local! {
        static KEYS: RefCell<HashMap<String, &'static str>> = RefCell::new(HashMap::new());
    }
    let mode = if light { "light" } else { "dark" };
    let key = format!("float-theme-role-{role}-{mode}");
    KEYS.with(|keys| {
        *keys
            .borrow_mut()
            .entry(key.clone())
            .or_insert_with(|| Box::leak(key.into_boxed_str()))
    })
}

fn rgb_hex(hex: &str) -> gpui::Rgba {
    let clean = hex.trim().trim_start_matches('#');
    let parsed = u32::from_str_radix(clean, 16).unwrap_or(0x86d3f8);
    gpui::rgb(parsed)
}

enum UndoEntry {
    Patch(Map<String, Value>),
    ThemeRpc(String),
}

pub(crate) struct GpuiThemeEditorWindow {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    scroll: ScrollHandle,
    focus_handle: FocusHandle,
    scheme_dark: bool,
    picked_dark: bool,
    picked_light: bool,
    all_themes: bool,
    artworks_scroll: ScrollHandle,
    artworks_wheel: StripWheelLock,
    artworks_revealed: Option<bool>,
    artworks_reveal: Rc<Cell<Option<usize>>>,
    more_colours_contrast: bool,
    split_areas: bool,
    more_roles: bool,
    more_glass: bool,
    busy_theme: Option<String>,
    undo_stack: Vec<UndoEntry>,
    slider_undo_armed: Rc<RefCell<HashSet<&'static str>>>,
    drag_press: Option<Point<Pixels>>,
    posters: HashMap<String, Arc<Image>>,
    source_images: HashMap<String, Arc<RenderImage>>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsPage for GpuiThemeEditorWindow {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

impl GpuiThemeEditorWindow {
    pub(crate) fn new(
        config: SettingsModalConfig,
        host: SettingsModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = SettingsPalette::resolve(&config.palette);
        let open_forms = OpenFormMemory::from_stored(&config.open_forms);
        let store = new_settings_store(
            host,
            palette,
            config.request,
            config.sidebar_state,
            open_forms,
            cx,
        );
        let values = store.read(cx).values();
        let scheme_dark = !matches!(
            window.appearance(),
            WindowAppearance::Light | WindowAppearance::VibrantLight
        );
        let sidebar = values.f64("themeSidebarContrast");
        let split_areas = sidebar != values.f64("themeWorkAreaContrast")
            || colourfulness_step_index(&values).is_none();
        let store_sub = cx.observe(&store, |_, _, cx| cx.notify());
        let appearance_sub = cx.observe_window_appearance(window, |editor, window, cx| {
            let next_dark = !matches!(
                window.appearance(),
                WindowAppearance::Light | WindowAppearance::VibrantLight
            );
            if editor.scheme_dark != next_dark {
                editor.scheme_dark = next_dark;
                cx.notify();
            }
        });
        let close_store = store.clone();
        window.on_window_should_close(cx, move |_window, cx| {
            close_store.update(cx, |store, cx| store.close(cx));
            true
        });
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);
        Self {
            store,
            fields: FieldStates::default(),
            scroll: ScrollHandle::new(),
            focus_handle,
            scheme_dark,
            picked_dark: false,
            picked_light: false,
            all_themes: false,
            artworks_scroll: ScrollHandle::new(),
            artworks_wheel: StripWheelLock::default(),
            artworks_revealed: None,
            artworks_reveal: Rc::new(Cell::new(None)),
            more_colours_contrast: split_areas,
            split_areas,
            more_roles: false,
            more_glass: false,
            busy_theme: None,
            undo_stack: Vec::new(),
            slider_undo_armed: Rc::new(RefCell::new(HashSet::new())),
            drag_press: None,
            posters: HashMap::new(),
            source_images: HashMap::new(),
            _subscriptions: vec![store_sub, appearance_sub],
        }
    }

    pub(crate) fn receive_sidebar_state(&mut self, message: Value, cx: &mut Context<Self>) {
        let store = self.store.clone();
        store.update(cx, |store, cx| store.receive_sidebar_state(message, cx));
    }

    pub(crate) fn receive_palette(&mut self, palette: &ModalPalette, cx: &mut Context<Self>) {
        let resolved = SettingsPalette::resolve(palette);
        let store = self.store.clone();
        store.update(cx, |store, cx| store.receive_palette(resolved, cx));
    }

    pub(crate) fn receive_host_payload(&mut self, payload: Value, cx: &mut Context<Self>) {
        let store = self.store.clone();
        store.update(cx, |store, cx| store.receive_host_payload(payload, cx));
    }

    #[allow(dead_code)]
    pub(crate) fn open_custom_color_picker_for_demo(
        &mut self,
        dark: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pick_custom_colour(dark, window, cx);
    }

    fn close_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        let store = self.store.clone();
        store.update(cx, |store, cx| store.close(cx));
    }

    fn push_undo(&mut self, entry: UndoEntry) {
        if self.undo_stack.len() >= 25 {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(entry);
    }

    fn record_undo(&mut self, keys: &[&str], values: &SettingsValues) {
        let mut snapshot = Map::new();
        for key in keys {
            snapshot.insert((*key).to_string(), values.value(key));
        }
        if !snapshot.is_empty() {
            self.push_undo(UndoEntry::Patch(snapshot));
        }
    }

    fn tracked_slider_saver(
        &self,
        slider_id: &'static str,
        undo_keys: &'static [&'static str],
        apply: impl Fn(&mut SettingsStore, f64, bool, &mut Context<SettingsStore>) + 'static,
        cx: &mut Context<Self>,
    ) -> SliderSaver {
        let armed = self.slider_undo_armed.clone();
        let editor = cx.weak_entity();
        Rc::new(move |store, value, commit, cx| {
            let mut armed_set = armed.borrow_mut();
            let first_of_gesture = !armed_set.contains(slider_id);
            if first_of_gesture {
                let values = store.read(cx).values();
                let mut snapshot = Map::new();
                for key in undo_keys {
                    snapshot.insert((*key).to_string(), values.value(key));
                }
                if !snapshot.is_empty() {
                    let editor = editor.clone();
                    cx.defer(move |cx| {
                        let _ = editor.update(cx, |editor, cx| {
                            editor.push_undo(UndoEntry::Patch(snapshot));
                            cx.notify();
                        });
                    });
                }
                if !commit {
                    armed_set.insert(slider_id);
                }
            } else if commit {
                armed_set.remove(slider_id);
            }
            drop(armed_set);
            store.update(cx, |store, cx| apply(store, value, commit, cx));
        })
    }

    fn apply_tracked_patch(&mut self, patch: Map<String, Value>, cx: &mut Context<Self>) {
        if patch.is_empty() {
            return;
        }
        let values = self.store.read(cx).values();
        let keys: Vec<&str> = patch.keys().map(String::as_str).collect();
        self.record_undo(&keys, &values);
        let store = self.store.clone();
        store.update(cx, |store, cx| {
            store.apply_patch(patch, "settings:control", cx)
        });
    }

    fn save_one(&mut self, key: &str, value: Value, cx: &mut Context<Self>) {
        let mut patch = Map::new();
        patch.insert(key.to_string(), value);
        self.apply_tracked_patch(patch, cx);
    }

    fn undo(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.undo_stack.pop() else {
            return;
        };
        match entry {
            UndoEntry::ThemeRpc(change_id) => {
                if self.store.read(cx).request().gxserver_rpc_available {
                    self.theme_rpc(
                        "undo",
                        json!({ "op": "undo", "change": change_id }),
                        "Couldn't undo the theme",
                        cx,
                    );
                }
            }
            UndoEntry::Patch(snapshot) => {
                let store = self.store.clone();
                store.update(cx, |store, cx| {
                    store.apply_patch(snapshot, "settings:control", cx)
                });
            }
        }
    }

    fn reset_to_default(&mut self, cx: &mut Context<Self>) {
        self.apply_gallery_theme("default", cx);
    }

    fn theme_rpc(&mut self, id: &str, params: Value, failed: &'static str, cx: &mut Context<Self>) {
        if self.busy_theme.is_some() {
            return;
        }
        let is_undo = params["op"] == "undo";
        self.busy_theme = Some(id.to_string());
        cx.notify();
        let this = cx.weak_entity();
        let store = self.store.clone();
        store_gxserver_rpc(
            &self.store,
            THEME_ENDPOINT,
            params,
            THEME_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |editor, cx| {
                    editor.busy_theme = None;
                    match result {
                        Ok(_) if is_undo => {}
                        Ok(answer) => {
                            if let Some(change_id) = answer["change"]["changeId"]
                                .as_str()
                                .filter(|id| !id.is_empty())
                            {
                                editor.push_undo(UndoEntry::ThemeRpc(change_id.to_string()));
                            }
                        }
                        Err(message) => {
                            store.update(cx, |store, cx| {
                                store.toast("warning", failed, &message, cx)
                            });
                        }
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    fn apply_gallery_theme(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(theme) = gallery_themes().into_iter().find(|theme| theme.id == id) else {
            return;
        };
        let values = self.store.read(cx).values();
        let mut changes = theme.changes.clone();
        let partner = theme.kind == "preset" && (!theme.dark || !self.picked_light);
        if partner {
            changes.extend(theme.partner_changes.clone());
        }
        if let Some(background) = &theme.default_background {
            changes.extend(theme.background_patch(background, partner));
        }
        if theme.kind == "preset" {
            if theme.dark {
                self.picked_dark = true;
            } else {
                self.picked_light = true;
            }
        } else if !preview_scheme_dark(&values.string("sidebarTheme"), self.scheme_dark) {
            changes.insert("sidebarTheme".into(), json!("dark-2"));
        }
        if self.store.read(cx).request().gxserver_rpc_available {
            self.theme_rpc(
                id,
                json!({ "op": "set", "changes": changes }),
                "Theme not applied",
                cx,
            );
        } else {
            self.apply_direct_theme_changes(&changes, cx);
        }
    }

    /// Fallback for offline / preview binary where gxserver `/api/theme` is not running: converts
    /// catalog string values into a settings patch and applies directly to `SettingsStore`.
    fn apply_direct_theme_changes(&mut self, changes: &Map<String, Value>, cx: &mut Context<Self>) {
        let mut patch = Map::new();
        let mut role_map = self
            .store
            .read(cx)
            .values()
            .value(THEME_ROLES_SETTING)
            .as_object()
            .cloned()
            .unwrap_or_default();
        let mut touched_roles = false;
        for (key, value) in changes {
            if let Some((appearance @ ("dark" | "light"), role_id)) = key.split_once('.') {
                touched_roles = true;
                let entry = role_map
                    .entry(appearance.to_string())
                    .or_insert_with(|| Value::Object(Map::new()));
                if let Some(obj) = entry.as_object_mut() {
                    if value.as_str() == Some("default") {
                        obj.remove(role_id);
                    } else {
                        obj.insert(role_id.to_string(), value.clone());
                    }
                }
                continue;
            }
            if let Some(text) = value.as_str() {
                if let Some(default) = settings_catalog().default_value(key) {
                    if default.is_number()
                        && let Ok(num) = text.parse::<f64>()
                    {
                        patch.insert(key.clone(), json!(num));
                        continue;
                    }
                    if default.is_boolean()
                        && let Ok(flag) = text.parse::<bool>()
                    {
                        patch.insert(key.clone(), json!(flag));
                        continue;
                    }
                }
            }
            patch.insert(key.clone(), value.clone());
        }
        if touched_roles {
            patch.insert(THEME_ROLES_SETTING.to_string(), Value::Object(role_map));
        }
        self.apply_tracked_patch(patch, cx);
    }

    fn pick_no_artwork(&mut self, dark: bool, enable_transparency: bool, cx: &mut Context<Self>) {
        let values = self.store.read(cx).values();
        let mut changes = Map::new();
        changes.insert(art_key(dark).into(), json!("none"));
        if live_shows(&values, dark) {
            changes.insert("windowGlassSource".into(), json!("desktopAndWindows"));
        }
        if enable_transparency {
            let glass = values.string("windowGlass");
            if glass == "opaque" {
                changes.insert(
                    "windowGlass".into(),
                    json!(if dark { "auto" } else { "frosted" }),
                );
            } else if glass == "auto" && !dark {
                changes.insert("windowGlass".into(), json!("frosted"));
            }
        }
        if self.store.read(cx).request().gxserver_rpc_available {
            self.theme_rpc(
                "artwork-none",
                json!({ "op": "set", "changes": changes }),
                "Background not changed",
                cx,
            );
        } else {
            self.apply_tracked_patch(changes, cx);
        }
    }

    fn pick_live(&mut self, dark: bool, cx: &mut Context<Self>) {
        let values = self.store.read(cx).values();
        let glass = values.string("windowGlass");
        let mut changes = Map::new();
        if values.string(art_key(dark)) != "none" {
            changes.insert(art_key(dark).into(), json!("none"));
        }
        changes.insert("windowGlassSource".into(), json!("live"));
        if glass == "opaque" {
            changes.insert(
                "windowGlass".into(),
                json!(if dark { "auto" } else { "frosted" }),
            );
        } else if glass == "auto" && !dark {
            changes.insert("windowGlass".into(), json!("frosted"));
        }
        if self.store.read(cx).request().gxserver_rpc_available {
            self.theme_rpc(
                "artwork-live",
                json!({ "op": "set", "changes": changes }),
                "Background not changed",
                cx,
            );
        } else {
            self.apply_tracked_patch(changes, cx);
        }
    }

    fn pick_your_image(&mut self, dark: bool, change: bool, cx: &mut Context<Self>) {
        let values = self.store.read(cx).values();
        if change || your_image_path(&values, dark).is_none() {
            post_store_message(
                &self.store,
                json!({
                    "appearance": if dark { "dark" } else { "light" },
                    "type": "pickWindowArtImageFile",
                }),
                cx,
            );
            return;
        }
        let mut changes = Map::new();
        changes.insert(art_key(dark).into(), json!(YOUR_IMAGE_ART_ID));
        if self.store.read(cx).request().gxserver_rpc_available {
            self.theme_rpc(
                "artwork-yours",
                json!({ "op": "set", "changes": changes }),
                "Background not changed",
                cx,
            );
        } else {
            self.apply_tracked_patch(changes, cx);
        }
    }

    fn select_artwork(&mut self, id: &str, dark: bool, cx: &mut Context<Self>) {
        if self.store.read(cx).request().gxserver_rpc_available {
            self.theme_rpc(
                &format!("artwork-{id}"),
                json!({ "op": "setArtwork", "id": id, "light": !dark }),
                "Background not changed",
                cx,
            );
        } else {
            let mut patch = Map::new();
            patch.insert(art_key(dark).into(), json!(id));
            self.apply_tracked_patch(patch, cx);
        }
    }

    fn select_colour_square(&mut self, dark: bool, preset: String, cx: &mut Context<Self>) {
        let (own_key, other_key) = if dark {
            ("darkThemePreset", "lightThemePreset")
        } else {
            ("lightThemePreset", "darkThemePreset")
        };
        let mut patch = Map::new();
        let other_picked = if dark {
            self.picked_light
        } else {
            self.picked_dark
        };
        if !other_picked
            && preset != "custom"
            && let Some(paired) = paired_preset(dark, &preset)
        {
            patch.insert(other_key.into(), json!(paired));
        }
        patch.insert(own_key.into(), json!(preset));
        if dark {
            self.picked_dark = true;
        } else {
            self.picked_light = true;
        }
        self.apply_tracked_patch(patch, cx);
    }

    fn pick_custom_colour(&mut self, dark: bool, window: &mut Window, cx: &mut Context<Self>) {
        let tint_key = custom_tint_key(dark);
        let values = self.store.read(cx).values();
        let tint = values.string(tint_key);
        let own_key = if dark {
            "darkThemePreset"
        } else {
            "lightThemePreset"
        };
        if values.string(own_key) != "custom" {
            let mut snapshot = Map::new();
            snapshot.insert(own_key.to_string(), values.value(own_key));
            snapshot.insert(tint_key.to_string(), values.value(tint_key));
            self.push_undo(UndoEntry::Patch(snapshot));
            let mut patch = Map::new();
            patch.insert(own_key.to_string(), json!("custom"));
            if dark {
                self.picked_dark = true;
            } else {
                self.picked_light = true;
            }
            let store = self.store.clone();
            store.update(cx, |store, cx| {
                store.apply_patch(patch, "settings:control", cx)
            });
        }
        open_tint_picker(self, tint_key, &tint, window, cx);
        let pre_tint = self.store.read(cx).values().value(tint_key);
        let recorded = Rc::new(Cell::new(false));
        let editor = cx.weak_entity();
        let saver: ColorSaver = Rc::new(move |store, picked_hex, commit, cx| {
            let current = store.read(cx).values().string(tint_key);
            if current.eq_ignore_ascii_case(picked_hex) && !commit {
                return;
            }
            if !recorded.replace(true) && !current.eq_ignore_ascii_case(picked_hex) {
                let mut snapshot = Map::new();
                snapshot.insert(tint_key.to_string(), pre_tint.clone());
                let editor = editor.clone();
                cx.defer(move |cx| {
                    let _ = editor.update(cx, |editor, cx| {
                        editor.push_undo(UndoEntry::Patch(snapshot));
                        cx.notify();
                    });
                });
            }
            store.update(cx, |store, cx| {
                if commit {
                    store.update_setting(tint_key, json!(picked_hex), cx);
                } else {
                    store.update_setting_debounced(tint_key, json!(picked_hex), cx);
                }
            });
        });
        set_color_saver(self, tint_key, saver);
    }

    fn set_accent(&mut self, hex: &str, light: bool, cx: &mut Context<Self>) {
        let current = self.store.read(cx).values().value(THEME_ROLES_SETTING);
        let next = with_role(&current, light, "accent", Some(hex.to_string()));
        let next = with_role(&next, light, "focusRing", Some(hex.to_string()));
        self.save_one(THEME_ROLES_SETTING, next, cx);
    }

    fn set_role_value(
        &mut self,
        light: bool,
        role: &'static str,
        value: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let current = self.store.read(cx).values().value(THEME_ROLES_SETTING);
        let next = with_role(&current, light, role, value);
        if next != current {
            self.save_one(THEME_ROLES_SETTING, next, cx);
        }
    }

    fn open_role_picker(
        &mut self,
        role: &'static str,
        light: bool,
        hex: &str,
        default: Option<RoleValue>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = role_picker_key(role, light);
        open_tint_picker(self, key, hex, window, cx);
        let pre_roles = self.store.read(cx).values().value(THEME_ROLES_SETTING);
        let recorded = Rc::new(Cell::new(false));
        let editor = cx.weak_entity();
        let saver: ColorSaver = Rc::new(move |store, picked_hex, commit, cx| {
            let Ok(RoleValue::Colour { rgb, .. }) = parse_role_value(picked_hex) else {
                return;
            };
            let changed = store.update(cx, |store, cx| {
                let current = store.values().value(THEME_ROLES_SETTING);
                let alpha = saved_role(&current, light, role)
                    .or(default)
                    .map_or(1.0, alpha_of);
                let next = with_role(
                    &current,
                    light,
                    role,
                    Some(format_role_value(RoleValue::Colour { rgb, alpha })),
                );
                if next == current {
                    return false;
                }
                if commit {
                    store.update_setting(THEME_ROLES_SETTING, next, cx);
                } else {
                    store.update_setting_debounced(THEME_ROLES_SETTING, next, cx);
                }
                true
            });
            if changed && !recorded.replace(true) {
                let mut snapshot = Map::new();
                snapshot.insert(THEME_ROLES_SETTING.to_string(), pre_roles.clone());
                let editor = editor.clone();
                cx.defer(move |cx| {
                    let _ = editor.update(cx, |editor, cx| {
                        editor.push_undo(UndoEntry::Patch(snapshot));
                        cx.notify();
                    });
                });
            }
        });
        set_color_saver(self, key, saver);
    }

    fn cached_picture(
        &mut self,
        kind: &str,
        id: &str,
        bytes: fn(&str) -> Option<&'static [u8]>,
    ) -> Option<Arc<Image>> {
        let key = format!("{kind}-{id}");
        if let Some(picture) = self.posters.get(&key) {
            return Some(picture.clone());
        }
        let picture = Arc::new(Image::from_bytes(ImageFormat::Webp, bytes(id)?.to_vec()));
        self.posters.insert(key, picture.clone());
        Some(picture)
    }

    fn cached_live_poster(&mut self, style: &str, light: bool) -> Option<Arc<Image>> {
        let style = if style == "video" { "ink" } else { style };
        let key = format!("live-{style}-{}", if light { "light" } else { "dark" });
        if let Some(picture) = self.posters.get(&key) {
            return Some(picture.clone());
        }
        let picture = live_poster(style, light)?;
        self.posters.insert(key, picture.clone());
        Some(picture)
    }

    fn cached_source_art(&mut self, source: &str) -> Option<Arc<RenderImage>> {
        if let Some(image) = self.source_images.get(source) {
            return Some(image.clone());
        }
        let image = source_art(source)?;
        self.source_images.insert(source.to_string(), image.clone());
        Some(image)
    }

    fn strength_slider_row(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (exact, strength) = strength_from_settings(values);
        let strength_label = if exact.is_some() {
            format!("{}%", strength.round() as i64)
        } else {
            format!("~{}%", strength.round() as i64)
        };
        let strength_saver = self.tracked_slider_saver(
            "float-glass-strength",
            &[
                "windowGlassSidebarOpacityDark",
                "windowGlassWorkAreaTintDark",
                "windowGlassSidebarOpacityLight",
                "windowGlassWorkAreaTintLight",
            ],
            |store, value, _commit, cx| {
                let gaps = strength_gaps(&store.values());
                store.apply_patch(strength_patch(value, gaps), "settings:control", cx);
            },
            cx,
        );
        self.compact_slider_row(
            p,
            "Strength",
            strength_label,
            SliderBinding {
                key: "float-glass-strength",
                min: STRENGTH_MIN,
                max: STRENGTH_MAX,
                step: STRENGTH_STEP,
            },
            strength,
            strength_saver,
            window,
            cx,
        )
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape"
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
        {
            self.close_window(window, cx);
        }
    }

    // ---- compact UI building blocks ----------------------------------------------------------

    fn section_heading(p: &SettingsPalette, title: &str, readout: Option<String>) -> AnyElement {
        h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .gap(px(6.0))
            .mb(px(6.0))
            .child(
                div()
                    .text_size(px(10.5))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(hsla(p.muted))
                    .child(title.to_uppercase()),
            )
            .when_some(readout, |this, text| {
                this.child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(11.0))
                        .text_color(hsla(p.muted))
                        .child(text),
                )
            })
            .into_any_element()
    }

    fn section_box(p: &SettingsPalette, content: Vec<AnyElement>) -> AnyElement {
        v_flex()
            .w_full()
            .py(px(10.0))
            .border_b_1()
            .border_color(hsla(p.hairline))
            .children(content)
            .into_any_element()
    }

    fn segmented(
        &self,
        p: &SettingsPalette,
        id_prefix: &'static str,
        options: &[(&'static str, &'static str)],
        current: &str,
        on_pick: impl Fn(&mut Self, &'static str, &mut Window, &mut Context<Self>) + Clone + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let track_bg = hsla(p.foreground_alpha(0.06));
        let active_bg = hsla(p.foreground_alpha(if p.light { 0.14 } else { 0.16 }));
        let hover_bg = hsla(p.foreground_alpha(0.09));
        let fg = hsla(p.foreground);
        let muted = hsla(p.muted);
        h_flex()
            .w_full()
            .p(px(2.0))
            .gap(px(2.0))
            .rounded(px(7.0))
            .bg(track_bg)
            .children(options.iter().map(|&(value, label)| {
                let selected = value == current;
                let on_pick = on_pick.clone();
                div()
                    .id(SharedString::from(format!("{id_prefix}-{value}")))
                    .flex_1()
                    .min_w_0()
                    .h(px(24.0))
                    .px(px(4.0))
                    .rounded(px(5.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_size(px(11.5))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .when(selected, |this| {
                        this.bg(active_bg)
                            .text_color(fg)
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                    })
                    .when(!selected, |this| {
                        this.text_color(muted)
                            .hover(move |s| s.bg(hover_bg).text_color(fg))
                    })
                    .on_click(cx.listener(move |editor, _: &ClickEvent, window, cx| {
                        on_pick(editor, value, window, cx);
                    }))
                    .child(label)
            }))
            .into_any_element()
    }

    fn compact_slider_row(
        &mut self,
        p: &SettingsPalette,
        label: &'static str,
        readout: String,
        binding: SliderBinding,
        value: f64,
        saver: SliderSaver,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let track = step_slider(self, p, binding, value, saver, window, cx);
        v_flex()
            .w_full()
            .gap(px(4.0))
            .pt(px(6.0))
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(hsla(p.foreground))
                            .child(label),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(hsla(p.muted))
                            .child(readout),
                    ),
            )
            .child(div().w_full().child(track))
            .into_any_element()
    }

    fn fold_button(
        &self,
        p: &SettingsPalette,
        id: &'static str,
        label: &'static str,
        open: bool,
        on_toggle: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let fg = hsla(p.foreground);
        let muted = hsla(p.muted);
        h_flex()
            .id(id)
            .w_full()
            .mt(px(6.0))
            .py(px(4.0))
            .gap(px(5.0))
            .items_center()
            .cursor_pointer()
            .text_size(px(11.5))
            .text_color(muted)
            .hover(move |s| s.text_color(fg))
            .on_click(cx.listener(move |editor, _: &ClickEvent, _window, cx| {
                on_toggle(editor, cx);
            }))
            .child(settings_icon(
                if open {
                    ICON_CHEVRON_DOWN
                } else {
                    ICON_CHEVRON_RIGHT
                },
                12.0,
                p.muted,
            ))
            .child(label)
            .into_any_element()
    }

    fn compact_toggle_row(
        &self,
        p: &SettingsPalette,
        id: &'static str,
        label: &'static str,
        checked: bool,
        on_toggle: impl Fn(&mut Self, bool, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let track_bg = if checked {
            hsla(p.foreground)
        } else {
            hsla(p.foreground_alpha(0.14))
        };
        let knob_bg = if checked {
            hsla(p.modal.solid_surface)
        } else {
            hsla(p.foreground_alpha(0.75))
        };
        h_flex()
            .id(id)
            .w_full()
            .pt(px(6.0))
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .cursor_pointer()
            .on_click(cx.listener(move |editor, _: &ClickEvent, _window, cx| {
                on_toggle(editor, !checked, cx);
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.0))
                    .text_color(hsla(p.foreground))
                    .child(label),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(30.0))
                    .h(px(17.0))
                    .rounded(px(9.0))
                    .bg(track_bg)
                    .p(px(2.0))
                    .flex()
                    .items_center()
                    .when(checked, |this| this.justify_end())
                    .child(div().size(px(13.0)).rounded_full().bg(knob_bg)),
            )
            .into_any_element()
    }

    // ---- sections ----------------------------------------------------------------------------

    fn header(&self, p: &SettingsPalette, cx: &mut Context<Self>) -> AnyElement {
        let hover_bg = hsla(p.foreground_alpha(0.08));
        let can_undo = !self.undo_stack.is_empty();
        let icon_btn =
            |id: &'static str, icon_path: &'static str, tip: &'static str, enabled: bool| {
                div()
                    .id(id)
                    .size(px(26.0))
                    .rounded(px(6.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(enabled, |this| {
                        this.cursor_pointer().hover(move |s| s.bg(hover_bg))
                    })
                    .when(!enabled, |this| this.opacity(0.35))
                    .tooltip(tooltip_text(tip))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(settings_icon(icon_path, 14.0, p.muted))
            };
        let drag_band = h_flex()
            .id("theme-editor-drag-header")
            .flex_1()
            .min_w_0()
            .h_full()
            .items_center()
            .gap(px(6.0))
            .cursor_grab()
            .window_control_area(WindowControlArea::Drag)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|editor, event: &gpui::MouseDownEvent, _window, _cx| {
                    editor.drag_press = Some(event.position);
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|editor, _, _, _| {
                    editor.drag_press = None;
                }),
            )
            .on_mouse_move(
                cx.listener(|editor, event: &gpui::MouseMoveEvent, window, _| {
                    let Some(press) = editor.drag_press else {
                        return;
                    };
                    if event.pressed_button != Some(MouseButton::Left) {
                        editor.drag_press = None;
                        return;
                    }
                    let dx = f32::from(event.position.x - press.x);
                    let dy = f32::from(event.position.y - press.y);
                    if dx.hypot(dy) >= DRAG_THRESHOLD {
                        editor.drag_press = None;
                        window.start_window_move();
                    }
                }),
            )
            .child(settings_icon(ICON_GRIP, 14.0, p.muted))
            .child(
                div()
                    .text_size(px(13.5))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(hsla(p.foreground))
                    .child("Theme"),
            );
        h_flex()
            .flex_shrink_0()
            .w_full()
            .h(px(HEADER_HEIGHT))
            .px(px(10.0))
            .gap(px(4.0))
            .items_center()
            .border_b_1()
            .border_color(hsla(p.hairline))
            .child(drag_band)
            .child(
                icon_btn("theme-editor-undo", ICON_UNDO, "Undo last change", can_undo).on_click(
                    cx.listener(move |editor, _: &ClickEvent, _window, cx| {
                        if can_undo {
                            editor.undo(cx);
                        }
                    }),
                ),
            )
            .child(
                icon_btn("theme-editor-close", ICON_CLOSE, "Close theme editor", true).on_click(
                    cx.listener(|editor, _: &ClickEvent, window, cx| {
                        editor.close_window(window, cx);
                    }),
                ),
            )
            .into_any_element()
    }

    fn footer(&self, p: &SettingsPalette, cx: &mut Context<Self>) -> AnyElement {
        let fg = hsla(p.foreground);
        let muted = hsla(p.muted);
        h_flex()
            .flex_shrink_0()
            .w_full()
            .h(px(FOOTER_HEIGHT))
            .px(px(12.0))
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .border_t_1()
            .border_color(hsla(p.hairline))
            .child(
                h_flex()
                    .id("theme-editor-all-settings")
                    .gap(px(5.0))
                    .items_center()
                    .cursor_pointer()
                    .text_size(px(11.5))
                    .text_color(muted)
                    .hover(move |s| s.text_color(fg))
                    .on_click(cx.listener(|editor, _: &ClickEvent, _window, cx| {
                        let store = editor.store.clone();
                        store.update(cx, |store, cx| store.open_theme_settings(cx));
                    }))
                    .child(settings_icon(ICON_SETTINGS, 12.5, p.muted))
                    .child("All theme settings"),
            )
            .child(
                h_flex()
                    .id("theme-editor-reset")
                    .gap(px(4.0))
                    .items_center()
                    .cursor_pointer()
                    .text_size(px(11.5))
                    .text_color(muted)
                    .hover(move |s| s.text_color(fg))
                    .on_click(cx.listener(|editor, _: &ClickEvent, _window, cx| {
                        editor.reset_to_default(cx);
                    }))
                    .child(settings_icon(ICON_RESET, 11.5, p.muted))
                    .child("Reset"),
            )
            .into_any_element()
    }

    fn themes_grid_section(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let themes: Vec<GalleryTheme> = gallery_themes();
        let presets = theme_presets();
        let active_id: Option<String> = applied_theme(&themes, values)
            .map(|(t, _, _)| t.id.clone())
            .or_else(|| {
                presets
                    .iter()
                    .find(|preset| theme_preset_matches(values, preset))
                    .map(|preset| preset.id.clone())
            });
        let active_name = active_id
            .as_deref()
            .and_then(|id| themes.iter().find(|t| t.id == id))
            .map(|t| t.name.clone())
            .unwrap_or_else(|| "Custom colour".to_string());
        let total = themes.len();
        let count = if self.all_themes {
            total
        } else {
            INITIAL_THEME_TILES.min(total)
        };
        let mut tiles: Vec<AnyElement> = Vec::with_capacity(count);
        for theme in themes.iter().take(count) {
            let id = theme.id.clone();
            let label = theme.label.clone().unwrap_or_else(|| {
                theme
                    .name
                    .split_whitespace()
                    .next()
                    .unwrap_or(&theme.name)
                    .to_string()
            });
            let selected = active_id.as_deref() == Some(id.as_str());
            let picture = self.cached_picture("theme-preview", &id, preview_bytes);
            let ring_color = if selected {
                hsla(p.foreground)
            } else {
                hsla(p.hairline)
            };
            let title = theme.name.clone();
            tiles.push(
                v_flex()
                    .id(SharedString::from(format!("float-theme-tile-{id}")))
                    .w(px(68.0))
                    .gap(px(3.0))
                    .cursor_pointer()
                    .tooltip(tooltip_text(title))
                    .on_click(cx.listener(move |editor, _: &ClickEvent, _window, cx| {
                        editor.apply_gallery_theme(&id, cx);
                    }))
                    .child(
                        div()
                            .w_full()
                            .h(px(40.0))
                            .rounded(px(6.0))
                            .overflow_hidden()
                            .bg(hsla(p.raised))
                            .border_color(ring_color)
                            .when(selected, |this| this.border_2())
                            .when(!selected, |this| this.border_1())
                            .when_some(picture, |this, pic| {
                                this.child(img(pic).size_full().object_fit(ObjectFit::Cover))
                            }),
                    )
                    .child(
                        div()
                            .w_full()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(10.5))
                            .text_color(if selected {
                                hsla(p.foreground)
                            } else {
                                hsla(p.muted)
                            })
                            .when(selected, |this| {
                                this.font_weight(gpui::FontWeight::SEMIBOLD)
                            })
                            .child(label),
                    )
                    .into_any_element(),
            );
        }
        let mut content = vec![
            Self::section_heading(p, "Theme", Some(active_name)),
            h_flex()
                .w_full()
                .flex_wrap()
                .gap(px(6.0))
                .children(tiles)
                .into_any_element(),
        ];
        if total > INITIAL_THEME_TILES {
            let expanded = self.all_themes;
            let fg = hsla(p.foreground);
            let muted = hsla(p.muted);
            content.push(
                h_flex()
                    .id("float-theme-toggle-all")
                    .mt(px(6.0))
                    .gap(px(4.0))
                    .items_center()
                    .cursor_pointer()
                    .text_size(px(11.5))
                    .text_color(muted)
                    .hover(move |s| s.text_color(fg))
                    .on_click(cx.listener(move |editor, _: &ClickEvent, _window, cx| {
                        editor.all_themes = !expanded;
                        cx.notify();
                    }))
                    .child(settings_icon(
                        if expanded {
                            ICON_CHEVRON_DOWN
                        } else {
                            ICON_CHEVRON_RIGHT
                        },
                        12.0,
                        p.muted,
                    ))
                    .child(if expanded {
                        "Show fewer".to_string()
                    } else {
                        format!("Show all {total}")
                    })
                    .into_any_element(),
            );
        }
        Self::section_box(p, content)
    }

    fn artwork_strip_tile(
        p: &SettingsPalette,
        id: SharedString,
        title: String,
        tip: String,
        selected: bool,
        preview: AnyElement,
        on_click: impl Fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        v_flex()
            .id(id)
            .w(px(68.0))
            .flex_shrink_0()
            .gap(px(3.0))
            .cursor_pointer()
            .tooltip(tooltip_text(tip))
            .on_click(cx.listener(on_click))
            .child(
                div()
                    .w_full()
                    .h(px(36.0))
                    .rounded(px(5.0))
                    .overflow_hidden()
                    .bg(hsla(p.raised))
                    .border_color(if selected {
                        hsla(p.foreground)
                    } else {
                        hsla(p.hairline)
                    })
                    .when(selected, |this| this.border_2())
                    .when(!selected, |this| this.border_1())
                    .child(preview),
            )
            .child(
                div()
                    .w_full()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(10.0))
                    .text_color(if selected {
                        hsla(p.foreground)
                    } else {
                        hsla(p.muted)
                    })
                    .when(selected, |this| {
                        this.font_weight(gpui::FontWeight::SEMIBOLD)
                    })
                    .child(title),
            )
            .into_any_element()
    }

    fn artwork_section(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        dark: bool,
        picked: &Backdrop,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let labels: Vec<(String, String)> = gallery_themes()
            .into_iter()
            .map(|theme| {
                let label = theme.label.clone().unwrap_or_else(|| {
                    theme
                        .name
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_string()
                });
                (theme.id, label)
            })
            .collect();
        let active_name = match picked {
            Backdrop::None => "None".to_string(),
            Backdrop::Artwork(id) => artwork_by_id(id)
                .map(|a| a.name.to_string())
                .unwrap_or_else(|| (*id).to_string()),
            Backdrop::YourImage => "Your image".to_string(),
            Backdrop::Live => "Animated".to_string(),
        };

        let mut tiles: Vec<AnyElement> = Vec::with_capacity(ARTWORKS.len() + 3);
        tiles.push(Self::artwork_strip_tile(
            p,
            "float-artwork-none".into(),
            "None".into(),
            "No artwork".into(),
            *picked == Backdrop::None,
            div().size_full().bg(hsla(p.raised)).into_any_element(),
            move |editor, _, _, cx| editor.pick_no_artwork(dark, false, cx),
            cx,
        ));
        for artwork in ARTWORKS {
            let id = artwork.id;
            let short = labels
                .iter()
                .find(|(theme_id, _)| theme_id == id)
                .map(|(_, label)| label.clone())
                .unwrap_or_else(|| artwork.name.to_string());
            let picture = self.cached_picture("theme-art", id, artwork_thumbnail_bytes);
            let preview = div()
                .size_full()
                .when_some(picture, |this, pic| {
                    this.child(img(pic).size_full().object_fit(ObjectFit::Cover))
                })
                .into_any_element();
            tiles.push(Self::artwork_strip_tile(
                p,
                SharedString::from(format!("float-art-{id}")),
                short,
                artwork.name.to_string(),
                *picked == Backdrop::Artwork(id),
                preview,
                move |editor, _, _, cx| editor.select_artwork(id, dark, cx),
                cx,
            ));
        }

        // Your image…
        let path = your_image_path(values, dark);
        let yours_picked = *picked == Backdrop::YourImage;
        let yours_preview = match path.clone() {
            Some(path) => file_preview(path, false),
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(hsla(p.foreground_alpha(0.04)))
                .child(settings_icon(ICON_PLUS, 13.0, p.muted))
                .into_any_element(),
        };
        tiles.push(Self::artwork_strip_tile(
            p,
            "float-artwork-yours".into(),
            "Your image".into(),
            "Your image: any picture behind the window".into(),
            yours_picked,
            yours_preview,
            move |editor, _, _, cx| editor.pick_your_image(dark, yours_picked, cx),
            cx,
        ));

        // Animated at the end of the Artwork strip.
        let active_style = values.string(if dark || values.string("windowGlass") == "auto" {
            "windowGlassLiveStyleDark"
        } else {
            "windowGlassLiveStyleLight"
        });
        let live_pic = self.cached_live_poster(&active_style, !dark);
        let live_preview = div()
            .size_full()
            .when_some(live_pic, |this, pic| {
                this.child(img(pic).size_full().object_fit(ObjectFit::Cover))
            })
            .into_any_element();
        tiles.push(Self::artwork_strip_tile(
            p,
            "float-artwork-live".into(),
            "Animated".into(),
            "Animated: a calm animation in your theme colours, or your own video".into(),
            *picked == Backdrop::Live,
            live_preview,
            move |editor, _, _, cx| editor.pick_live(dark, cx),
            cx,
        ));

        let picked_slot = match picked {
            Backdrop::None => None,
            Backdrop::Artwork(id) => ARTWORKS
                .iter()
                .position(|artwork| artwork.id == *id)
                .map(|index| index + 1),
            Backdrop::YourImage => Some(ARTWORKS.len() + 1),
            Backdrop::Live => Some(ARTWORKS.len() + 2),
        };
        if let Some(slot) = picked_slot
            && self.artworks_revealed != Some(dark)
        {
            self.artworks_revealed = Some(dark);
            self.artworks_reveal.set(Some(slot));
        }
        let scroll = self.artworks_scroll.clone();
        let wheel = strip_wheel_layer(&scroll, &self.artworks_wheel);
        let reveal = self.artworks_reveal.clone();
        let measure = scroll.clone();
        let entity = cx.entity_id();

        let strip = div()
            .relative()
            .w_full()
            .min_w_0()
            .child(
                div()
                    .id("float-artworks-strip")
                    .w_full()
                    .flex()
                    .items_start()
                    .overflow_x_scroll()
                    .restrict_scroll_to_axis()
                    .track_scroll(&scroll)
                    .gap(px(6.0))
                    .pb(px(2.0))
                    .children(tiles),
            )
            .child(wheel)
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, _, _, cx| {
                        if let Some(slot) = reveal.get()
                            && let Some(tile) = measure.bounds_for_item(slot)
                        {
                            reveal.set(None);
                            let view = measure.bounds();
                            let centre = tile.left() - view.left() + tile.size.width / 2.0;
                            let target = (view.size.width / 2.0 - centre)
                                .clamp(-measure.max_offset().x, px(0.0));
                            measure.set_offset(point(target, measure.offset().y));
                            cx.notify(entity);
                        }
                    },
                )
                .absolute()
                .size_0(),
            )
            .into_any_element();

        let mut content = vec![
            Self::section_heading(p, "Artwork", Some(active_name)),
            strip,
        ];

        match picked {
            Backdrop::Artwork(_) | Backdrop::YourImage => {
                if *picked == Backdrop::YourImage && path.is_none() {
                    let hover_bg = hsla(p.foreground_alpha(0.10));
                    content.push(
                        h_flex()
                            .w_full()
                            .pt(px(6.0))
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(11.0))
                                    .text_color(hsla(p.muted))
                                    .child("No picture chosen yet."),
                            )
                            .child(
                                div()
                                    .id("float-artwork-choose-pic")
                                    .px(px(8.0))
                                    .py(px(3.0))
                                    .rounded(px(5.0))
                                    .border_1()
                                    .border_color(hsla(p.hairline))
                                    .bg(hsla(p.foreground_alpha(0.05)))
                                    .hover(move |s| s.bg(hover_bg))
                                    .cursor_pointer()
                                    .text_size(px(11.0))
                                    .text_color(hsla(p.foreground))
                                    .on_click(cx.listener(move |editor, _, _, cx| {
                                        editor.pick_your_image(dark, true, cx);
                                    }))
                                    .child("Choose picture…"),
                            )
                            .into_any_element(),
                    );
                }
                content.push(self.strength_slider_row(p, values, window, cx));
                let dim = values.f64("windowArtDim");
                let dim_saver = self.tracked_slider_saver(
                    "float-art-dim",
                    &["windowArtDim"],
                    |store, value, commit, cx| {
                        if commit {
                            store.update_setting("windowArtDim", json!(value.round()), cx);
                        } else {
                            store.update_setting_debounced(
                                "windowArtDim",
                                json!(value.round()),
                                cx,
                            );
                        }
                    },
                    cx,
                );
                content.push(self.compact_slider_row(
                    p,
                    "Dimming",
                    format!("{}%", dim.round() as i64),
                    SliderBinding {
                        key: "float-art-dim",
                        min: settings_catalog().number(module::SETTINGS, "MIN_WINDOW_ART_DIM"),
                        max: settings_catalog().number(module::SETTINGS, "MAX_WINDOW_ART_DIM"),
                        step: 1.0,
                    },
                    dim,
                    dim_saver,
                    window,
                    cx,
                ));
                let blur = values.f64("windowArtBlur");
                let blur_saver = self.tracked_slider_saver(
                    "float-art-blur",
                    &["windowArtBlur"],
                    |store, value, commit, cx| {
                        if commit {
                            store.update_setting("windowArtBlur", json!(value.round()), cx);
                        } else {
                            store.update_setting_debounced(
                                "windowArtBlur",
                                json!(value.round()),
                                cx,
                            );
                        }
                    },
                    cx,
                );
                content.push(self.compact_slider_row(
                    p,
                    "Blur",
                    format!("{} pt", blur.round() as i64),
                    SliderBinding {
                        key: "float-art-blur",
                        min: settings_catalog().number(module::SETTINGS, "MIN_WINDOW_ART_BLUR"),
                        max: settings_catalog().number(module::SETTINGS, "MAX_WINDOW_ART_BLUR"),
                        step: 1.0,
                    },
                    blur,
                    blur_saver,
                    window,
                    cx,
                ));
            }
            Backdrop::Live => {
                let single =
                    values.string("windowGlass") == "auto" || same_for_light_and_dark(values);
                let live_key = if dark || single {
                    "windowGlassLiveStyleDark"
                } else {
                    "windowGlassLiveStyleLight"
                };
                let current_live = values.string(live_key);
                let options =
                    settings_catalog().options(module::SETTINGS, "WINDOW_GLASS_LIVE_STYLE_OPTIONS");
                let mut cards: Vec<AnyElement> = Vec::with_capacity(options.len());
                for option in options {
                    let style_id = option.value;
                    let style_name = option.label;
                    let selected = current_live == style_id;
                    let picture = self.cached_live_poster(&style_id, !dark);
                    cards.push(
                        v_flex()
                            .id(SharedString::from(format!("float-live-{style_id}")))
                            .w(px(68.0))
                            .gap(px(2.0))
                            .cursor_pointer()
                            .tooltip(tooltip_text(style_name.clone()))
                            .on_click(cx.listener(move |editor, _: &ClickEvent, _window, cx| {
                                let mut patch = Map::new();
                                patch.insert(live_key.into(), json!(style_id));
                                if single {
                                    patch.insert(
                                        "windowGlassLiveStyleLight".into(),
                                        json!(style_id),
                                    );
                                }
                                patch.insert("windowGlassSource".into(), json!("live"));
                                if editor.store.read(cx).values().string("windowGlass") == "opaque"
                                {
                                    patch.insert(
                                        "windowGlass".into(),
                                        json!(if dark { "auto" } else { "frosted" }),
                                    );
                                }
                                editor.apply_tracked_patch(patch, cx);
                            }))
                            .child(
                                div()
                                    .w_full()
                                    .h(px(34.0))
                                    .rounded(px(5.0))
                                    .overflow_hidden()
                                    .bg(hsla(p.raised))
                                    .border_color(if selected {
                                        hsla(p.foreground)
                                    } else {
                                        hsla(p.hairline)
                                    })
                                    .when(selected, |this| this.border_2())
                                    .when(!selected, |this| this.border_1())
                                    .when_some(picture, |this, pic| {
                                        this.child(
                                            img(pic).size_full().object_fit(ObjectFit::Cover),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(if selected {
                                        hsla(p.foreground)
                                    } else {
                                        hsla(p.muted)
                                    })
                                    .when(selected, |this| {
                                        this.font_weight(gpui::FontWeight::SEMIBOLD)
                                    })
                                    .child(style_name),
                            )
                            .into_any_element(),
                    );
                }
                content.push(
                    h_flex()
                        .w_full()
                        .mt(px(8.0))
                        .flex_wrap()
                        .gap(px(6.0))
                        .children(cards)
                        .into_any_element(),
                );
                content.push(self.strength_slider_row(p, values, window, cx));
                let speed = values.f64("windowGlassLiveSpeed");
                let speed_label = if (speed - speed.round()).abs() < 1e-6 {
                    format!("{}x", speed as i64)
                } else {
                    format!("{speed:.2}x").replace(".50x", ".5x")
                };
                let speed_saver = self.tracked_slider_saver(
                    "float-live-speed",
                    &["windowGlassLiveSpeed"],
                    |store, value, commit, cx| {
                        let rounded = (value * 4.0).round() / 4.0;
                        if commit {
                            store.update_setting("windowGlassLiveSpeed", json!(rounded), cx);
                        } else {
                            store.update_setting_debounced(
                                "windowGlassLiveSpeed",
                                json!(rounded),
                                cx,
                            );
                        }
                    },
                    cx,
                );
                content.push(
                    self.compact_slider_row(
                        p,
                        "Speed",
                        speed_label,
                        SliderBinding {
                            key: "float-live-speed",
                            min: settings_catalog()
                                .number(module::SETTINGS, "MIN_WINDOW_GLASS_LIVE_SPEED"),
                            max: settings_catalog()
                                .number(module::SETTINGS, "MAX_WINDOW_GLASS_LIVE_SPEED"),
                            step: 0.25,
                        },
                        speed,
                        speed_saver,
                        window,
                        cx,
                    ),
                );
                let bright = values.f64("windowGlassLiveBrightness");
                let bright_saver = self.tracked_slider_saver(
                    "float-live-brightness",
                    &["windowGlassLiveBrightness"],
                    |store, value, commit, cx| {
                        if commit {
                            store.update_setting(
                                "windowGlassLiveBrightness",
                                json!(value.round()),
                                cx,
                            );
                        } else {
                            store.update_setting_debounced(
                                "windowGlassLiveBrightness",
                                json!(value.round()),
                                cx,
                            );
                        }
                    },
                    cx,
                );
                content.push(
                    self.compact_slider_row(
                        p,
                        "Brightness",
                        format!("{}%", bright.round() as i64),
                        SliderBinding {
                            key: "float-live-brightness",
                            min: settings_catalog()
                                .number(module::SETTINGS, "MIN_WINDOW_GLASS_LIVE_BRIGHTNESS"),
                            max: settings_catalog()
                                .number(module::SETTINGS, "MAX_WINDOW_GLASS_LIVE_BRIGHTNESS"),
                            step: 1.0,
                        },
                        bright,
                        bright_saver,
                        window,
                        cx,
                    ),
                );
                let plugged = values.bool("windowGlassVideoOnlyOnPower");
                content.push(self.compact_toggle_row(
                    p,
                    "float-live-power",
                    "Play only when plugged in",
                    plugged,
                    |editor, checked, cx| {
                        editor.save_one("windowGlassVideoOnlyOnPower", json!(checked), cx)
                    },
                    cx,
                ));
            }
            Backdrop::None => {}
        }
        Self::section_box(p, content)
    }

    fn colour_section(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        dark: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let preset_key = if dark {
            "darkThemePreset"
        } else {
            "lightThemePreset"
        };
        let selected = values.string(preset_key);
        let readout = if selected == "custom" {
            "Custom".to_string()
        } else {
            preset_label(dark, &selected)
        };
        let step = colourfulness_display_step(values.f64("themeSidebarContrast"));
        let mut squares: Vec<AnyElement> = preset_options(dark)
            .into_iter()
            .filter(|(value, _)| value != "custom")
            .map(|(value, label)| {
                let colour = swatch_colour(&swatch_paint(dark, &value, step));
                let is_selected = value == selected;
                let preset = value.clone();
                div()
                    .id(SharedString::from(format!("float-sq-{value}")))
                    .size(px(21.0))
                    .rounded(px(5.0))
                    .cursor_pointer()
                    .bg(gpui::Hsla::from(colour.gpui()))
                    .border_color(if is_selected {
                        hsla(p.foreground)
                    } else {
                        hsla(p.foreground_alpha(0.14))
                    })
                    .when(is_selected, |this| this.border_2())
                    .when(!is_selected, |this| this.border_1())
                    .tooltip(tooltip_text(label))
                    .on_click(cx.listener(move |editor, _: &ClickEvent, _window, cx| {
                        editor.select_colour_square(dark, preset.clone(), cx);
                    }))
                    .into_any_element()
            })
            .collect();
        let tint_key = custom_tint_key(dark);
        let tint = values.string(tint_key);
        let custom_selected = selected == "custom";
        squares.push(
            div()
                .id(SharedString::from(format!(
                    "float-sq-custom-{}",
                    if dark { "dark" } else { "light" }
                )))
                .relative()
                .size(px(21.0))
                .rounded(px(5.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .when(custom_selected, |this| {
                    this.bg(gpui::Hsla::from(rgb_hex(&tint)))
                        .border_2()
                        .border_color(hsla(p.foreground))
                })
                .when(!custom_selected, |this| {
                    this.border_1()
                        .border_dashed()
                        .border_color(hsla(p.foreground_alpha(0.4)))
                })
                .tooltip(tooltip_text("Custom colour (uncapped)"))
                .on_click(cx.listener(move |editor, _: &ClickEvent, window, cx| {
                    editor.pick_custom_colour(dark, window, cx);
                }))
                .child(settings_icon(
                    ICON_PLUS,
                    11.0,
                    if custom_selected {
                        p.foreground
                    } else {
                        p.muted
                    },
                ))
                .into_any_element(),
        );
        let mut content = vec![
            Self::section_heading(
                p,
                if dark {
                    "Dark mode colour"
                } else {
                    "Light mode colour"
                },
                Some(readout),
            ),
            h_flex()
                .w_full()
                .flex_wrap()
                .gap(px(6.0))
                .children(squares)
                .into_any_element(),
        ];
        let cf_pos = colourfulness_step_index(values).unwrap_or(step);
        let cf_name = colourfulness_name(colourfulness_points(cf_pos)).to_string();
        let cf_saver = self.tracked_slider_saver(
            "float-colourfulness",
            &[
                "themeSidebarContrast",
                "themeWorkAreaContrast",
                "customSidebarTitlebarBackgroundDarknessPercent",
                "customSidebarTitlebarLightBackgroundLightnessPercent",
            ],
            |store, value, _commit, cx| {
                let pos = value.round().clamp(0.0, COLOURFULNESS_LAST_POSITION as f64) as usize;
                if colourfulness_step_index(&store.values()) != Some(pos) {
                    store.apply_patch(colourfulness_patch(pos), "settings:control", cx);
                }
            },
            cx,
        );
        content.push(self.compact_slider_row(
            p,
            "Colourfulness",
            if self.split_areas {
                "Per area".to_string()
            } else {
                cf_name
            },
            SliderBinding {
                key: "float-colourfulness",
                min: 0.0,
                max: COLOURFULNESS_LAST_POSITION as f64,
                step: 1.0,
            },
            cf_pos as f64,
            cf_saver,
            window,
            cx,
        ));
        let more_open = self.more_colours_contrast;
        content.push(self.fold_button(
            p,
            "float-more-contrast",
            "Sidebar & work area contrast",
            more_open,
            |editor, cx| {
                editor.more_colours_contrast = !editor.more_colours_contrast;
                cx.notify();
            },
            cx,
        ));
        if more_open {
            let split = self.split_areas;
            content.push(self.compact_toggle_row(
                p,
                "float-split-areas",
                "Set sidebar and work area apart",
                split,
                |editor, checked, cx| {
                    editor.split_areas = checked;
                    if !checked {
                        let sidebar = editor.store.read(cx).values().f64("themeSidebarContrast");
                        editor.apply_tracked_patch(
                            colourfulness_patch(colourfulness_display_step(sidebar)),
                            cx,
                        );
                    }
                    cx.notify();
                },
                cx,
            ));
            if split {
                for (key, slider_key, label, undo_keys) in [
                    (
                        "themeSidebarContrast",
                        "float-sb-contrast",
                        "Sidebar contrast",
                        &[
                            "themeSidebarContrast",
                            "customSidebarTitlebarBackgroundDarknessPercent",
                            "customSidebarTitlebarLightBackgroundLightnessPercent",
                        ][..],
                    ),
                    (
                        "themeWorkAreaContrast",
                        "float-wa-contrast",
                        "Work area contrast",
                        &["themeWorkAreaContrast"][..],
                    ),
                ] {
                    let pos = colourfulness_display_step(values.f64(key));
                    let name = colourfulness_name(colourfulness_points(pos)).to_string();
                    let saver = self.tracked_slider_saver(
                        slider_key,
                        undo_keys,
                        move |store, value, _commit, cx| {
                            let position =
                                value.round().clamp(0.0, COLOURFULNESS_LAST_POSITION as f64)
                                    as usize;
                            if store.values().f64(key) != colourfulness_points(position) {
                                store.apply_patch(
                                    area_colourfulness_patch(key, position),
                                    "settings:control",
                                    cx,
                                );
                            }
                        },
                        cx,
                    );
                    content.push(self.compact_slider_row(
                        p,
                        label,
                        name,
                        SliderBinding {
                            key: slider_key,
                            min: 0.0,
                            max: COLOURFULNESS_LAST_POSITION as f64,
                            step: 1.0,
                        },
                        pos as f64,
                        saver,
                        window,
                        cx,
                    ));
                }
            }
        }
        Self::section_box(p, content)
    }

    fn accent_and_roles_section(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        light: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let saved_roles = values.value(THEME_ROLES_SETTING);
        let base_rgb = ghostex_settings_catalog::theme_colours::work_area_background_for_variant(
            values.map(),
            light,
        );
        let current_accent = saved_role(&saved_roles, light, "accent")
            .or_else(|| role_default(&saved_roles, "accent", light))
            .map(|value| hex_of(value, light, base_rgb))
            .unwrap_or_else(|| "#86d3f8".to_string());
        let mut dots: Vec<AnyElement> = ACCENT_DOTS
            .iter()
            .map(|&hex| {
                let selected = current_accent.eq_ignore_ascii_case(hex);
                div()
                    .id(SharedString::from(format!("float-accent-{hex}")))
                    .size(px(19.0))
                    .rounded_full()
                    .cursor_pointer()
                    .bg(gpui::Hsla::from(rgb_hex(hex)))
                    .border_color(if selected {
                        hsla(p.foreground)
                    } else {
                        hsla(p.foreground_alpha(0.18))
                    })
                    .when(selected, |this| this.border_2())
                    .when(!selected, |this| this.border_1())
                    .tooltip(tooltip_text(hex))
                    .on_click(cx.listener(move |editor, _: &ClickEvent, _window, cx| {
                        editor.set_accent(hex, light, cx);
                    }))
                    .into_any_element()
            })
            .collect();
        let accent_hex_for_click = current_accent.clone();
        dots.push(
            div()
                .id("float-accent-custom")
                .relative()
                .size(px(19.0))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .border_1()
                .border_dashed()
                .border_color(hsla(p.foreground_alpha(0.45)))
                .tooltip(tooltip_text("Pick custom accent"))
                .on_click(cx.listener(move |editor, _: &ClickEvent, window, cx| {
                    editor.open_role_picker(
                        "accent",
                        light,
                        &accent_hex_for_click,
                        None,
                        window,
                        cx,
                    );
                }))
                .child(settings_icon(ICON_PLUS, 10.0, p.muted))
                .into_any_element(),
        );
        let mut content = vec![
            Self::section_heading(p, "Accent & colours", Some(current_accent)),
            h_flex()
                .w_full()
                .flex_wrap()
                .gap(px(7.0))
                .items_center()
                .children(dots)
                .into_any_element(),
        ];
        let roles_open = self.more_roles;
        content.push(self.fold_button(
            p,
            "float-more-roles",
            "Fine-tune colour roles",
            roles_open,
            |editor, cx| {
                editor.more_roles = !editor.more_roles;
                cx.notify();
            },
            cx,
        ));
        if roles_open {
            for &(role_id, label) in &FINE_TUNE_ROLES {
                let value = saved_role(&saved_roles, light, role_id);
                let default = role_default(&saved_roles, role_id, light);
                let shown = value.or(default);
                let hex = shown
                    .map(|val| hex_of(val, light, base_rgb))
                    .unwrap_or_else(|| "#808080".to_string());
                let fill = shown.map(|val| {
                    let [r, g, b, a] = val.rgba_on_base(light, base_rgb);
                    gpui::Hsla::from(gpui::Rgba { r, g, b, a })
                });
                let display_text = value
                    .map(format_role_value)
                    .unwrap_or_else(|| "default".to_string());
                let click_hex = hex.clone();
                let is_set = value.is_some();
                let muted = hsla(p.muted);
                let fg = hsla(p.foreground);
                content.push(
                    h_flex()
                        .w_full()
                        .py(px(3.0))
                        .gap(px(6.0))
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(11.5))
                                .text_color(fg)
                                .child(label),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .max_w(px(96.0))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(10.5))
                                .text_color(muted)
                                .child(display_text),
                        )
                        .when(is_set, |this| {
                            this.child(
                                div()
                                    .id(SharedString::from(format!("float-role-reset-{role_id}")))
                                    .flex_shrink_0()
                                    .size(px(18.0))
                                    .rounded(px(4.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .tooltip(tooltip_text("Reset role"))
                                    .on_click(cx.listener(
                                        move |editor, _: &ClickEvent, _window, cx| {
                                            editor.set_role_value(light, role_id, None, cx);
                                        },
                                    ))
                                    .child(settings_icon(ICON_RESET, 10.0, p.muted)),
                            )
                        })
                        .child(
                            div()
                                .id(SharedString::from(format!("float-role-sw-{role_id}")))
                                .flex_shrink_0()
                                .relative()
                                .size(px(18.0))
                                .rounded(px(4.0))
                                .cursor_pointer()
                                .border_1()
                                .border_color(hsla(p.hairline))
                                .bg(hsla(p.surface))
                                .when_some(fill, |this, fill| {
                                    this.child(div().size_full().rounded(px(3.0)).bg(fill))
                                })
                                .tooltip(tooltip_text("Pick colour"))
                                .on_click(cx.listener(
                                    move |editor, _: &ClickEvent, window, cx| {
                                        editor.open_role_picker(
                                            role_id, light, &click_hex, default, window, cx,
                                        );
                                    },
                                )),
                        )
                        .into_any_element(),
                );
            }
        }
        Self::section_box(p, content)
    }

    /// Renders whichever Pick Color dialog is currently open (custom tint, accent, or fine-tune
    /// role) centered inside the floating Theme Editor window with zero clipping.
    fn active_color_picker_overlay(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        dark: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let light = !dark;
        let tint_key = custom_tint_key(dark);
        if self
            .fields
            .colors
            .get(tint_key)
            .is_some_and(|state| state.picker.open)
        {
            let tint = values.string(tint_key);
            return centered_tint_picker_overlay(
                self,
                p,
                tint_key,
                &tint,
                THEME_EDITOR_WIDTH,
                window,
                cx,
            );
        }
        let saved_roles = values.value(THEME_ROLES_SETTING);
        let base_rgb = ghostex_settings_catalog::theme_colours::work_area_background_for_variant(
            values.map(),
            light,
        );
        let accent_key = role_picker_key("accent", light);
        if self
            .fields
            .colors
            .get(accent_key)
            .is_some_and(|state| state.picker.open)
        {
            let hex = saved_role(&saved_roles, light, "accent")
                .or_else(|| role_default(&saved_roles, "accent", light))
                .map(|val| hex_of(val, light, base_rgb))
                .unwrap_or_else(|| "#86d3f8".to_string());
            return centered_tint_picker_overlay(
                self,
                p,
                accent_key,
                &hex,
                THEME_EDITOR_WIDTH,
                window,
                cx,
            );
        }
        for &(role_id, _) in &FINE_TUNE_ROLES {
            let key = role_picker_key(role_id, light);
            if self
                .fields
                .colors
                .get(key)
                .is_some_and(|state| state.picker.open)
            {
                let hex = saved_role(&saved_roles, light, role_id)
                    .or_else(|| role_default(&saved_roles, role_id, light))
                    .map(|val| hex_of(val, light, base_rgb))
                    .unwrap_or_else(|| "#808080".to_string());
                return centered_tint_picker_overlay(
                    self,
                    p,
                    key,
                    &hex,
                    THEME_EDITOR_WIDTH,
                    window,
                    cx,
                );
            }
        }
        None
    }

    fn window_transparency_section(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        dark: bool,
        picked: &Backdrop,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(name) = painted_name(values, dark) {
            let hover_bg = hsla(p.foreground_alpha(0.10));
            return Self::section_box(
                p,
                vec![
                    Self::section_heading(p, "Window transparency", Some("Paused".into())),
                    v_flex()
                        .w_full()
                        .gap(px(8.0))
                        .p(px(9.0))
                        .rounded(px(7.0))
                        .border_1()
                        .border_color(hsla(p.hairline))
                        .bg(hsla(p.foreground_alpha(0.03)))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .line_height(px(15.5))
                                .text_color(hsla(p.muted))
                                .child(format!(
                                    "{name} is behind the window in {} mode, so the window is solid and transparency is paused.",
                                    if dark { "dark" } else { "light" }
                                )),
                        )
                        .child(
                            div()
                                .id("float-transparency-resume")
                                .self_start()
                                .px(px(9.0))
                                .py(px(4.0))
                                .rounded(px(6.0))
                                .border_1()
                                .border_color(hsla(p.hairline))
                                .bg(hsla(p.foreground_alpha(0.06)))
                                .hover(move |s| s.bg(hover_bg))
                                .cursor_pointer()
                                .text_size(px(11.5))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(hsla(p.foreground))
                                .on_click(cx.listener(move |editor, _, _, cx| {
                                    editor.pick_no_artwork(dark, true, cx);
                                }))
                                .child("Use transparency"),
                        )
                        .into_any_element(),
                ],
            );
        }

        let glass = values.string("windowGlass");
        let source = values.string("windowGlassSource");
        let choices = transparency_choices();
        let active_bg = hsla(p.foreground_alpha(if p.light { 0.14 } else { 0.16 }));
        let hover_bg = hsla(p.foreground_alpha(0.09));
        let fg = hsla(p.foreground);
        let muted = hsla(p.muted);
        let glass_bar = h_flex()
            .w_full()
            .p(px(2.0))
            .gap(px(2.0))
            .rounded(px(7.0))
            .bg(hsla(p.foreground_alpha(0.06)))
            .children(choices.into_iter().map(|opt| {
                let selected = opt.value == glass;
                let value = opt.value;
                div()
                    .id(SharedString::from(format!("float-glass-{value}")))
                    .flex_1()
                    .min_w_0()
                    .h(px(24.0))
                    .px(px(4.0))
                    .rounded(px(5.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_size(px(11.5))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .when(selected, |this| {
                        this.bg(active_bg)
                            .text_color(fg)
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                    })
                    .when(!selected, |this| {
                        this.text_color(muted)
                            .hover(move |s| s.bg(hover_bg).text_color(fg))
                    })
                    .on_click(cx.listener(move |editor, _: &ClickEvent, _window, cx| {
                        editor.save_one("windowGlass", json!(value), cx);
                    }))
                    .child(opt.label)
            }))
            .into_any_element();

        let mut content = vec![
            Self::section_heading(p, "Window transparency", None),
            glass_bar,
        ];

        let active_here = glass != "opaque" && !(glass == "auto" && !dark);
        if active_here {
            let mut tiles: Vec<AnyElement> = Vec::with_capacity(SHOW_BEHIND.len());
            for &(value, title, sub_text) in &SHOW_BEHIND {
                let selected = source == value;
                let swatch = self.cached_source_art(value);
                let border_hover = hsla(p.foreground_alpha(0.3));
                tiles.push(
                    v_flex()
                        .id(SharedString::from(format!("float-show-behind-{value}")))
                        .flex_1()
                        .min_w_0()
                        .gap(px(5.0))
                        .p(px(6.0))
                        .rounded(px(7.0))
                        .border_color(hsla(if selected {
                            p.foreground_alpha(0.85)
                        } else {
                            p.hairline
                        }))
                        .when(selected, |this| this.border_2())
                        .when(!selected, |this| {
                            this.border_1().hover(move |s| s.border_color(border_hover))
                        })
                        .bg(hsla(p.foreground_alpha(0.03)))
                        .cursor_pointer()
                        .tooltip(tooltip_text(format!("{title}: {sub_text}")))
                        .on_click(cx.listener(move |editor, _: &ClickEvent, _window, cx| {
                            editor.save_one("windowGlassSource", json!(value), cx);
                        }))
                        .child(
                            div()
                                .relative()
                                .w_full()
                                .h(px(26.0))
                                .rounded(px(4.0))
                                .overflow_hidden()
                                .when_some(swatch, |this, image| {
                                    this.child(
                                        img(image)
                                            .absolute()
                                            .inset_0()
                                            .size_full()
                                            .object_fit(ObjectFit::Fill),
                                    )
                                })
                                .when(value == "desktopAndWindows", |this| {
                                    this.child(
                                        div()
                                            .absolute()
                                            .left(px(5.0))
                                            .top(px(5.0))
                                            .w(px(22.0))
                                            .h(px(14.0))
                                            .rounded(px(2.5))
                                            .bg(hsla(modal_rgba(0xffffff, 0.38))),
                                    )
                                }),
                        )
                        .child(
                            v_flex()
                                .w_full()
                                .min_w_0()
                                .gap(px(1.0))
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .line_height(px(13.5))
                                        .text_color(hsla(p.foreground))
                                        .when(selected, |this| {
                                            this.font_weight(gpui::FontWeight::SEMIBOLD)
                                        })
                                        .child(title),
                                )
                                .child(
                                    div()
                                        .text_size(px(9.5))
                                        .line_height(px(12.5))
                                        .text_color(hsla(p.muted))
                                        .child(sub_text),
                                ),
                        )
                        .into_any_element(),
                );
            }
            if source == "live" {
                content.push(
                    div()
                        .pt(px(6.0))
                        .text_size(px(11.0))
                        .line_height(px(14.5))
                        .text_color(hsla(p.muted))
                        .child("Animated is active in Artwork above. Pick one of these to see through the window instead.")
                        .into_any_element(),
                );
            }
            content.push(
                h_flex()
                    .w_full()
                    .pt(px(6.0))
                    .gap(px(6.0))
                    .children(tiles)
                    .into_any_element(),
            );

            if *picked == Backdrop::None {
                content.push(self.strength_slider_row(p, values, window, cx));
                let blur = values.f64("windowGlassBlurRadius");
                let blur_saver = self.tracked_slider_saver(
                    "float-glass-blur",
                    &["windowGlassBlurRadius"],
                    |store, value, commit, cx| {
                        if commit {
                            store.update_setting("windowGlassBlurRadius", json!(value.round()), cx);
                        } else {
                            store.update_setting_debounced(
                                "windowGlassBlurRadius",
                                json!(value.round()),
                                cx,
                            );
                        }
                    },
                    cx,
                );
                content.push(
                    self.compact_slider_row(
                        p,
                        "Blur",
                        format!("{} pt", blur.round() as i64),
                        SliderBinding {
                            key: "float-glass-blur",
                            min: settings_catalog()
                                .number(module::SETTINGS, "MIN_WINDOW_GLASS_BLUR_RADIUS"),
                            max: settings_catalog()
                                .number(module::SETTINGS, "MAX_WINDOW_GLASS_BLUR_RADIUS"),
                            step: 1.0,
                        },
                        blur,
                        blur_saver,
                        window,
                        cx,
                    ),
                );
            }
            let more_open = self.more_glass;
            content.push(self.fold_button(
                p,
                "float-more-glass",
                "More transparency",
                more_open,
                |editor, cx| {
                    editor.more_glass = !editor.more_glass;
                    cx.notify();
                },
                cx,
            ));
            if more_open {
                let menu_blur = values.f64("windowGlassMenuBlurRadius");
                let menu_blur_saver = self.tracked_slider_saver(
                    "float-menu-blur",
                    &["windowGlassMenuBlurRadius"],
                    |store, value, commit, cx| {
                        if commit {
                            store.update_setting(
                                "windowGlassMenuBlurRadius",
                                json!(value.round()),
                                cx,
                            );
                        } else {
                            store.update_setting_debounced(
                                "windowGlassMenuBlurRadius",
                                json!(value.round()),
                                cx,
                            );
                        }
                    },
                    cx,
                );
                content.push(
                    self.compact_slider_row(
                        p,
                        "Menu blur",
                        format!("{} pt", menu_blur.round() as i64),
                        SliderBinding {
                            key: "float-menu-blur",
                            min: settings_catalog()
                                .number(module::SETTINGS, "MIN_WINDOW_GLASS_MENU_BLUR_RADIUS"),
                            max: settings_catalog()
                                .number(module::SETTINGS, "MAX_WINDOW_GLASS_MENU_BLUR_RADIUS"),
                            step: 1.0,
                        },
                        menu_blur,
                        menu_blur_saver,
                        window,
                        cx,
                    ),
                );
            }
        } else if glass == "auto" && !dark {
            content.push(
                div()
                    .pt(px(6.0))
                    .text_size(px(11.0))
                    .line_height(px(14.5))
                    .text_color(hsla(p.muted))
                    .child("Transparency is set to Dark only. Pick Always for light mode too.")
                    .into_any_element(),
            );
        }
        Self::section_box(p, content)
    }

    fn chat_and_terminal_section(
        &self,
        p: &SettingsPalette,
        values: &SettingsValues,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chat = values.string("sessionChatTheme");
        let term = values.string("terminalColorScheme");
        v_flex()
            .w_full()
            .py(px(10.0))
            .gap(px(8.0))
            .child(Self::section_heading(p, "Chat & terminal", None))
            .child(
                v_flex()
                    .w_full()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(hsla(p.muted))
                            .child("Chat theme"),
                    )
                    .child(self.segmented(
                        p,
                        "float-chat-theme",
                        &[("auto", "Follow app"), ("dark", "Dark"), ("light", "Light")],
                        &chat,
                        |editor, value, _window, cx| {
                            editor.save_one("sessionChatTheme", json!(value), cx)
                        },
                        cx,
                    )),
            )
            .child(
                v_flex()
                    .w_full()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(hsla(p.muted))
                            .child("Terminal theme"),
                    )
                    .child(self.segmented(
                        p,
                        "float-term-theme",
                        &[("auto", "Follow app"), ("dark", "Dark"), ("light", "Light")],
                        &term,
                        |editor, value, _window, cx| {
                            editor.save_one("terminalColorScheme", json!(value), cx)
                        },
                        cx,
                    )),
            )
            .into_any_element()
    }
}

impl ModalCornerClose for GpuiThemeEditorWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window(window, cx);
    }

    fn shows_corner_close(&self, _cx: &App) -> bool {
        // The floating Theme Editor draws its own close control in its draggable header.
        false
    }

    fn keeps_open_on_click_away(&self, _window: &Window, _cx: &App) -> bool {
        true
    }
}

impl Focusable for GpuiThemeEditorWindow {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for GpuiThemeEditorWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.scheme_dark = !matches!(
            window.appearance(),
            WindowAppearance::Light | WindowAppearance::VibrantLight
        );
        let (p, values) = {
            let store = self.store.read(cx);
            (store.palette(), store.values())
        };
        let appearance = values.string("sidebarTheme");
        let dark = preview_scheme_dark(&appearance, self.scheme_dark);
        let picked = backdrop(&values, dark);
        let appearance_section = Self::section_box(
            &p,
            vec![
                Self::section_heading(&p, "Appearance", None),
                self.segmented(
                    &p,
                    "float-appearance",
                    &[
                        ("system", "System"),
                        ("plain-light", "Light"),
                        ("dark-2", "Dark"),
                    ],
                    &appearance,
                    |editor, value, _window, cx| {
                        editor.artworks_revealed = None;
                        editor.save_one("sidebarTheme", json!(value), cx);
                    },
                    cx,
                ),
            ],
        );
        let themes_section = self.themes_grid_section(&p, &values, cx);
        let artwork_section = self.artwork_section(&p, &values, dark, &picked, window, cx);
        let colour_section = self.colour_section(&p, &values, dark, window, cx);
        let accent_section = self.accent_and_roles_section(&p, &values, !dark, window, cx);
        let transparency_section =
            self.window_transparency_section(&p, &values, dark, &picked, window, cx);
        let chat_term_section = self.chat_and_terminal_section(&p, &values, cx);
        let color_picker_overlay = self.active_color_picker_overlay(&p, &values, dark, window, cx);
        v_flex()
            .id("ghostex-floating-theme-editor")
            .relative()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(hsla(p.surface))
            .text_color(hsla(p.foreground))
            .font_family(SETTINGS_FONT)
            .overflow_hidden()
            .on_key_down(cx.listener(Self::on_key_down))
            .child(self.header(&p, cx))
            .child(
                div()
                    .id("theme-editor-scroll-body")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .px(px(12.0))
                    .pb(px(12.0))
                    .child(
                        v_flex()
                            .w_full()
                            .child(appearance_section)
                            .child(themes_section)
                            .child(artwork_section)
                            .child(transparency_section)
                            .child(colour_section)
                            .child(accent_section)
                            .child(chat_term_section),
                    ),
            )
            .child(self.footer(&p, cx))
            .children(color_picker_overlay)
    }
}
