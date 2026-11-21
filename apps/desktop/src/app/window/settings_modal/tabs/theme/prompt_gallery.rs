//! The looks of the Themes page's Themes group and the way to ask an agent for one: Default, the six
//! art themes and the six agent-made looks (`theme_gallery` in
//! packages/settings-catalog/src/data/theme_gallery.rs) lead the themes row as tiles of the presets' own kind (`theme_tile` in presets.rs): a press applies
//! one, and its hover button, Make it your own, opens a Help chat with its prompt as the draft.
//! Under the row, the applied look's line (Undo, Change them) and Describe a look, whose Ask an
//! agent opens a Help chat with a starter prompt as its draft.
//!
//! CDXC:Theming 2026-11-20 DECISION:
//! User, of the presets row and the "Theme it in your words" card with its "A few starting points" gallery under it: "why do we have these 2 when they pretty much do very similar thing, pls fix". They are one gallery now: the six looks join the presets row first, as their own family captioned Agent-made, drawn like a preset with their picture as the preview; a press applies one through `/api/theme` `set` like `ghostex theme set` (one change in the theme history, Undo on the applied line until another theme is applied). Make it your own replaces Copy prompt, and the card's starter prompt is one Describe a look row. Supersedes the 2026-11-18 card of framed pictures with Apply and Copy prompt (docs/2026-11-18/theme-by-prompt/index.html).
//! SEE-ALSO: server/src/theme/endpoint.rs (the change and its Undo), apps/desktop/src/app/titlebar/help_menu.rs (`ask_ghostex_help`, the chat Ask an agent opens).
use super::super::super::super::native_modal_kit::*;
use super::super::super::catalog::{module, settings_catalog};
use super::super::super::fields::{
    ButtonVariant, SizedButtonSize, SizedButtonVariant, settings_icon, settings_icon_button,
    settings_sized_button,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::{SettingsValues, post_store_message, store_gxserver_rpc};
use super::ThemeTab;
use super::colours::preview_scheme_dark;
use super::presets::{TileSpec, theme_tile};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Image, ImageFormat, InteractiveElement as _, IntoElement, ObjectFit,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _, Window, div, img, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Map, Value, json};
use std::sync::Arc;
use std::time::Duration;

const DESCRIBE_TITLE: &str = "Describe a look";
const DESCRIBE_COPY: &str = "An agent sets the colours and glass from your words, like \u{201c}a calm, dark look with frosted glass\u{201d}. Your fonts stay as they are.";
/// The draft Ask an agent stages in the Help chat.
const STARTER_PROMPT: &str = "Give Ghostex a calm, dark look with soft colours and frosted glass.";
/// The captions over the first art theme and the first agent-made look in the themes row.
const ART_CAPTION: &str = "Art";
const LOOKS_CAPTION: &str = "Agent-made";
const THEME_ENDPOINT: &str = "/api/theme";
/// `/api/theme` waits up to 15s for the app to save, then 3s for the file to show it.
const THEME_TIMEOUT: Duration = Duration::from_secs(20);

/// One theme of the row (`theme_gallery` in packages/settings-catalog/src/data/themes.rs).
#[derive(Clone, Debug)]
pub(super) struct GalleryTheme {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) prompt: String,
    /// The word under its tile, when it is not the name's first word.
    pub(super) label: Option<String>,
    /// `default`, `art`, `look` or `preset`.
    pub(super) kind: String,
    /// An art theme: its own artwork is its first background.
    pub(super) art: bool,
    pub(super) dark: bool,
    pub(super) changes: Map<String, Value>,
    /// A preset's other appearance, added when it follows (presets.rs).
    pub(super) partner_changes: Map<String, Value>,
    /// The backgrounds it offers (art, live, transparent, solid) and the one it starts with.
    pub(super) backgrounds: Vec<String>,
    pub(super) default_background: Option<String>,
    pub(super) background_changes: Map<String, Value>,
    pub(super) partner_background_changes: Map<String, Value>,
}

impl GalleryTheme {
    /// What `background` changes, with the partner's when `partner` follows.
    pub(super) fn background_patch(&self, background: &str, partner: bool) -> Map<String, Value> {
        let mut changes = Map::new();
        let mut add = |from: &Map<String, Value>| {
            if let Some(from) = from.get(background).and_then(Value::as_object) {
                changes.extend(from.clone());
            }
        };
        add(&self.background_changes);
        if partner {
            add(&self.partner_background_changes);
        }
        changes
    }
}

/// The row's themes in the catalog's order: Default, the art themes, the agent-made looks, the
/// presets.
pub(super) fn gallery_themes() -> Vec<GalleryTheme> {
    let text = |entry: &Value, key: &str| {
        entry
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let object = |entry: &Value, key: &str| {
        entry
            .get(key)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    };
    settings_catalog()
        .module_value(module::SETTINGS, "THEME_GALLERY")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .map(|entry| GalleryTheme {
                    id: text(entry, "id"),
                    name: text(entry, "name"),
                    prompt: text(entry, "prompt"),
                    label: entry
                        .get("label")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    kind: text(entry, "kind"),
                    art: entry.get("art").and_then(Value::as_bool) == Some(true),
                    dark: text(entry, "mode") != "light",
                    changes: object(entry, "changes"),
                    partner_changes: object(entry, "partnerChanges"),
                    backgrounds: entry
                        .get("backgrounds")
                        .and_then(Value::as_array)
                        .map(|list| {
                            list.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default(),
                    default_background: entry
                        .get("defaultBackground")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    background_changes: object(entry, "backgroundChanges"),
                    partner_background_changes: object(entry, "partnerBackgroundChanges"),
                })
                .collect()
        })
        .unwrap_or_default()
}

macro_rules! bundled {
    ($dir:literal, $name:literal) => {
        include_bytes!(concat!(
            "../../../../../../assets/modals/settings/",
            $dir,
            "/",
            $name,
            ".webp"
        ))
    };
}

macro_rules! bundled_table {
    ($dir:literal; $($name:literal),* $(,)?) => {
        |id: &str| -> Option<&'static [u8]> {
            match id {
                $($name => Some(bundled!($dir, $name)),)*
                _ => None,
            }
        }
    };
}

/// The screenshot of theme `id` in the app (rendered from the web build in that theme with its
/// artwork behind it, tooling/theme-previews/), its tile's preview.
///
/// CDXC:Theming 2026-11-21 DECISION:
/// User: "i want all the themes to have a screenshot preview please". Every tile of the row shows the app itself in that theme, rendered from the web build, in place of the drawn preview cards the presets had and the pictures the agent-made looks had.
fn preview_bytes(id: &str) -> Option<&'static [u8]> {
    let table = bundled_table!("theme-previews";
        "default", "moonwater", "glasshouse", "red-earth", "deep-current", "night-garden",
        "nocturne-canyon", "firefly-forest", "ember-forge", "synthwave-84", "polar-night",
        "inkstone", "orchid-dusk", "mocha", "macchiato", "frappe", "latte", "dracula", "alucard",
        "sol-dark", "sol-light", "tn-night", "tn-storm", "tn-moon", "tn-day", "nord", "kg-wave",
        "kg-dragon", "kg-lotus", "ef-dark", "ef-light", "gm-dark", "gm-light",
    );
    table(id)
}

/// The small thumbnail of artwork `id` (`ARTWORKS`), for the Backgrounds gallery; the artwork
/// itself downloads when it is shown (helpers/window_art.rs).
fn artwork_thumbnail_bytes(id: &str) -> Option<&'static [u8]> {
    let table = bundled_table!("theme-art";
        "moonwater", "glasshouse", "red-earth", "deep-current", "night-garden", "nocturne-canyon",
        "inkstone", "firefly-forest", "ember-forge", "synthwave-84", "polar-night", "orchid-dusk",
        "mocha", "macchiato", "frappe", "latte", "dracula", "alucard", "sol-dark", "sol-light",
        "tn-night", "tn-storm", "tn-moon", "tn-day", "nord", "kg-wave", "kg-dragon", "kg-lotus",
        "ef-dark", "ef-light", "gm-dark", "gm-light",
    );
    table(id)
}

/// What the gallery is doing: the theme whose Apply or Undo is on its way, and the last theme
/// applied here with its change, which its tile can undo.
#[derive(Default)]
pub(super) struct PromptGallery {
    busy: Option<String>,
    applied: Option<(String, String)>,
    /// The Backgrounds gallery's sideways scroll.
    artworks_scroll: gpui::ScrollHandle,
}

/// One short line of a failed theme call for its notice: the first sentence of the answer's first
/// line. gxserver's answer can list one line per refused setting, which is for agents, not a notice.
fn short_reason(message: &str) -> String {
    let line = message
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Ghostex didn\u{2019}t answer.");
    match line.find(". ") {
        Some(end) => line[..=end].to_string(),
        None => line.to_string(),
    }
}

/// The theme whose colours are the ones set now (every theme colour it sets, "default" meaning
/// none saved), with how many it sets and whether Your messages is one. Of the themes that match,
/// the one most of whose other settings still hold wins (Default and Inkstone share colours). The
/// presets are left to `theme_preset_matches` (presets.rs), which also knows presets applied before
/// they set colours.
fn applied_theme<'a>(
    themes: &'a [GalleryTheme],
    values: &SettingsValues,
) -> Option<(&'a GalleryTheme, usize, bool)> {
    use ghostex_settings_catalog::theme_roles::{format_role_value, parse_role_value};
    let roles = values.value("themeRoles");
    let same = |saved: Option<&str>, wanted: &str| match (saved, wanted) {
        (None, "default") => true,
        (Some(a), b) => match (parse_role_value(a), parse_role_value(b)) {
            (Ok(a), Ok(b)) => format_role_value(a) == format_role_value(b),
            _ => false,
        },
        (None, _) => false,
    };
    let mut best: Option<(usize, (&GalleryTheme, usize, bool))> = None;
    for theme in themes.iter().filter(|theme| theme.kind != "preset") {
        let mut colours = 0;
        let mut mine = false;
        let mut others = 0;
        let mut matches = true;
        for (key, value) in &theme.changes {
            let wanted = value.as_str().unwrap_or_default();
            if let Some((appearance @ ("dark" | "light"), role)) = key.split_once('.') {
                let saved = roles
                    .get(appearance)
                    .and_then(|roles| roles.get(role))
                    .and_then(Value::as_str);
                if !same(saved, wanted) {
                    matches = false;
                    break;
                }
                if wanted != "default" {
                    colours += 1;
                    mine |= role == "userMessage";
                }
                continue;
            }
            let now = match values.value(key) {
                Value::String(text) => text,
                Value::Null => String::new(),
                other => other.to_string(),
            };
            if now.eq_ignore_ascii_case(wanted) {
                others += 1;
            }
        }
        if matches && colours > 0 && best.is_none_or(|(top, _)| others > top) {
            best = Some((others, (theme, colours, mine)));
        }
    }
    best.map(|(_, found)| found)
}

/// "Orchid Dusk is applied. It set 16 colours, your messages included.", Undo while it is the
/// look last applied here, and Change them, which opens the colours it set (CDXC:Theming 2026-11-18
/// in packages/settings-catalog/src/theme_roles.rs, Q3981 "both").
fn applied_line(
    p: &SettingsPalette,
    themes: &[GalleryTheme],
    values: &SettingsValues,
    gallery: &PromptGallery,
    cx: &mut Context<ThemeTab>,
) -> Option<AnyElement> {
    let (theme, count, mine) = applied_theme(themes, values)?;
    let undo = gallery
        .applied
        .as_ref()
        .is_some_and(|(id, _)| *id == theme.id)
        .then(|| {
            settings_sized_button(
                p,
                "theme-look-undo",
                "Undo",
                None,
                None,
                SizedButtonVariant::Secondary,
                SizedButtonSize::Sm,
                gallery.busy.is_some(),
                None,
                |page: &mut ThemeTab, _window, cx| page.undo_gallery_theme(cx),
                cx,
            )
        });
    let colours = if count == 1 {
        "1 colour".to_string()
    } else {
        format!("{count} colours")
    };
    let detail = if mine {
        format!("It set {colours}, your messages included.")
    } else {
        format!("It set {colours}.")
    };
    Some(
        h_flex()
            .w_full()
            .gap(px(16.0))
            .px(px(16.0))
            .py(px(12.0))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(19.0))
                            .text_color(hsla(p.foreground))
                            .child(format!("{} is applied", theme.name)),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(18.57))
                            .text_color(hsla(p.muted))
                            .child(detail),
                    ),
            )
            .children(undo)
            .child(settings_sized_button(
                p,
                "theme-prompt-change-colours",
                "Change them",
                None,
                None,
                SizedButtonVariant::Secondary,
                SizedButtonSize::Sm,
                false,
                None,
                |page: &mut ThemeTab, _window, cx| page.show_theme_colours(cx),
                cx,
            ))
            .into_any_element(),
    )
}

impl ThemeTab {
    /// Change them: Chat colours in view, and More theme colours open under it.
    fn show_theme_colours(&mut self, cx: &mut Context<Self>) {
        self.set_more_open(super::MoreGroup::ThemeColours, true, cx);
        let store = self.store.clone();
        store.update(cx, |store, cx| {
            store.scroll_to_section(
                super::super::super::model::SettingsTabId::Theme,
                "chatColours",
                cx,
            )
        });
    }

    /// Asks an agent: a Help chat opens with `draft` as its draft, and Settings closes so the chat
    /// shows. The window is removed here, as the shell's own close does (settings_modal/shell.rs):
    /// the store's close only tells the app the modal is gone.
    fn ask_agent_to_theme(&mut self, draft: &str, window: &mut Window, cx: &mut Context<Self>) {
        post_store_message(
            &self.store,
            json!({ "type": "askGhostexHelp", "question": draft }),
            cx,
        );
        window.remove_window();
        let store = self.store.clone();
        store.update(cx, |store, cx| store.close(cx));
    }

    /// Applies theme `id` as one theme change, with the background it starts with (a preset's is
    /// the one showing now). A dark theme that is not a preset switches an app showing light mode
    /// to Dark; a preset sets its own appearance and brings its partner along the way presets.rs
    /// says.
    pub(super) fn apply_gallery_theme(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(theme) = gallery_themes().into_iter().find(|theme| theme.id == id) else {
            return;
        };
        // No theme names a font (CDXC:Theming 2026-11-21 in theme_gallery.rs): the user's fonts stay.
        let mut changes = theme.changes.clone();
        let values = self.store.read(cx).values();
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
        self.theme_call(
            id,
            json!({ "op": "set", "changes": changes }),
            "Theme not applied",
            cx,
        );
    }

    /// Undoes the change the gallery last applied.
    fn undo_gallery_theme(&mut self, cx: &mut Context<Self>) {
        let Some((id, change)) = self.prompt_gallery.applied.clone() else {
            return;
        };
        self.theme_call(
            &id,
            json!({ "op": "undo", "change": change }),
            "Couldn\u{2019}t undo the theme",
            cx,
        );
    }

    /// One `/api/theme` call for theme `id`'s tile; a failure shows as a notice titled `failed`.
    fn theme_call(
        &mut self,
        id: &str,
        params: Value,
        failed: &'static str,
        cx: &mut Context<Self>,
    ) {
        if self.prompt_gallery.busy.is_some() {
            return;
        }
        let undo = params["op"] == "undo";
        self.prompt_gallery.busy = Some(id.to_string());
        cx.notify();
        let this = cx.weak_entity();
        let store = self.store.clone();
        let id = id.to_string();
        store_gxserver_rpc(
            &self.store,
            THEME_ENDPOINT,
            params,
            THEME_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    page.prompt_gallery.busy = None;
                    match result {
                        Ok(_) if undo => page.prompt_gallery.applied = None,
                        Ok(answer) => {
                            if let Some(change) = answer["change"]["changeId"]
                                .as_str()
                                .filter(|id| !id.is_empty())
                            {
                                page.prompt_gallery.applied = Some((id, change.to_string()));
                            }
                        }
                        Err(message) => {
                            let reason = short_reason(&message);
                            store
                                .update(cx, |store, cx| store.toast("warning", failed, &reason, cx))
                        }
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// Theme `id`'s screenshot, decoded once per page.
    pub(super) fn gallery_picture(&mut self, id: &str) -> Option<Arc<Image>> {
        self.bundled_picture("theme-preview", id, preview_bytes)
    }

    /// Artwork `id`'s thumbnail, decoded once per page.
    fn artwork_thumbnail(&mut self, id: &str) -> Option<Arc<Image>> {
        self.bundled_picture("theme-art", id, artwork_thumbnail_bytes)
    }

    fn bundled_picture(
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

    /// The looks' tiles for the themes row, each with whether it is the applied one. `cut` is the
    /// row's bits of the tiles an edge cuts; the looks are its first tiles.
    pub(super) fn look_tiles(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        cut: u64,
        cx: &mut Context<Self>,
    ) -> Vec<(bool, AnyElement)> {
        let themes: Vec<GalleryTheme> = gallery_themes()
            .into_iter()
            .filter(|theme| theme.kind != "preset")
            .collect();
        let applied = applied_theme(&themes, values).map(|(theme, _, _)| theme.id.clone());
        themes
            .iter()
            .enumerate()
            .map(|(index, theme)| {
                let selected = applied.as_deref() == Some(theme.id.as_str());
                // Default leads the row on its own; the art themes and the agent-made looks each
                // get a caption over their first tile.
                let previous = index.checked_sub(1).map(|previous| &themes[previous]);
                let first_of_kind = theme.id != ghostex_settings_catalog::DEFAULT_LOOK_ID
                    && previous.is_none_or(|previous| {
                        previous.art != theme.art
                            || previous.id == ghostex_settings_catalog::DEFAULT_LOOK_ID
                    });
                let caption = first_of_kind.then(|| {
                    if theme.art {
                        ART_CAPTION
                    } else {
                        LOOKS_CAPTION
                    }
                    .to_string()
                });
                let tile = self.look_tile(p, theme, caption, selected, cut & (1 << index) != 0, cx);
                (selected, tile)
            })
            .collect()
    }

    /// One look: its picture as the preview, its name's first word as the label, and Make it your
    /// own on hover. A press applies it, unless it is the one applied already.
    fn look_tile(
        &mut self,
        p: &SettingsPalette,
        theme: &GalleryTheme,
        caption: Option<String>,
        selected: bool,
        cut: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = self.prompt_gallery.busy.as_deref() == Some(theme.id.as_str());
        let preview = self
            .gallery_picture(&theme.id)
            .map(|picture| screenshot_preview(picture, busy))
            .into_iter()
            .collect();
        let draft = format!("{} Then change this: ", theme.prompt);
        // It floats over the picture, so it sits on the popups' own surface to stay readable on a
        // dark picture in light mode.
        let own = click_shield(div().rounded(px(7.0)).bg(hsla(p.popup_background)).child(
            settings_icon_button(
                p,
                SharedString::from(format!("theme-look-own-{}", theme.id)),
                "modals/settings/sparkles.svg",
                12.0,
                22.0,
                ButtonVariant::Outline,
                Some("Make it your own".into()),
                false,
                move |page: &mut Self, window, cx| page.ask_agent_to_theme(&draft, window, cx),
                cx,
            ),
        ));
        let id = theme.id.clone();
        theme_tile(
            p,
            TileSpec {
                id: SharedString::from(format!("theme-look-{}", theme.id)),
                name: theme.name.clone().into(),
                label: theme
                    .label
                    .clone()
                    .unwrap_or_else(|| {
                        theme
                            .name
                            .split_whitespace()
                            .next()
                            .unwrap_or_default()
                            .to_string()
                    })
                    .into(),
                dark: true,
                caption,
                selected,
                cut,
            },
            preview,
            // Default is Ghostex's own look; there is nothing of it to make your own.
            (theme.id != ghostex_settings_catalog::DEFAULT_LOOK_ID).then_some(own),
            move |page, _window, cx| {
                if !selected {
                    page.apply_gallery_theme(&id, cx);
                }
            },
            cx,
        )
    }

    /// The theme the settings show now: an agent-made, art or Default look by its colours, else a
    /// preset by its settings (presets.rs).
    fn applied_gallery_theme(&self, values: &SettingsValues) -> Option<GalleryTheme> {
        let themes = gallery_themes();
        if let Some((theme, _, _)) = applied_theme(&themes, values) {
            return Some(theme.clone());
        }
        let preset = super::presets::theme_presets()
            .into_iter()
            .find(|preset| super::presets::theme_preset_matches(values, preset))?;
        themes.into_iter().find(|theme| theme.id == preset.id)
    }

    /// The background showing now in the appearance on screen: art, solid, live or transparent.
    fn current_background(&self, values: &SettingsValues) -> &'static str {
        let dark = preview_scheme_dark(&values.string("sidebarTheme"), self.scheme_dark);
        let art = values.string(if dark { "windowArt" } else { "windowArtLight" });
        if ghostex_settings_catalog::artwork_by_id(&art).is_some() {
            return "art";
        }
        match values.string("windowGlass").as_str() {
            "opaque" => "solid",
            "auto" if !dark => "solid",
            _ if values.string("windowGlassSource") == "live" => "live",
            _ => "transparent",
        }
    }

    /// Background, under the themes row: the applied theme's Art, Live, Transparent and Solid
    /// (those it has), the one showing now picked.
    ///
    /// CDXC:Theming 2026-11-21 DECISION:
    /// User: "after i pick a theme i can switch the presets". Every theme's backgrounds sit in one control under the row; a press switches only the background, as one theme change Undo puts back, and keeps the theme's colours.
    pub(super) fn background_row(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let theme = self.applied_gallery_theme(values)?;
        let options: Vec<super::super::super::catalog::SettingOption> =
            ghostex_settings_catalog::BACKGROUNDS
                .iter()
                .filter(|(id, _)| theme.backgrounds.iter().any(|offered| offered == id))
                .map(|(id, label)| super::super::super::catalog::SettingOption {
                    label: label.to_string(),
                    value: id.to_string(),
                })
                .collect();
        let current = self.current_background(values);
        let partner = theme.kind == "preset" && (!theme.dark || !self.picked_light);
        Some(super::super::super::fields::segmented_field(
            p,
            "theme-background",
            super::super::super::fields::RowSpec::new("Background".to_string()).description(format!(
                "What {} shows behind the sidebar and the work area: its own artwork, its Live animation, your desktop through the glass, or nothing.",
                theme.name
            )),
            None,
            &options,
            Some(current),
            None,
            move |page: &mut Self, next, _window, cx| {
                let changes = theme.background_patch(&next, partner);
                if changes.is_empty() {
                    return;
                }
                page.theme_call(
                    &format!("background-{next}"),
                    json!({ "op": "set", "changes": changes }),
                    "Background not changed",
                    cx,
                );
            },
            cx,
        ))
    }

    /// Backgrounds: every theme's artwork, to put behind any theme (in the appearance on screen),
    /// and None. Each downloads the first time it shows.
    ///
    /// CDXC:Theming 2026-11-21 DECISION:
    /// User: every theme image "is also a standalone background the user can pick for ANY theme: a background gallery (thumbnails, downloaded on demand like the rest) in Settings > Themes next to the presets, so the user can combine any theme with any image. The theme own image stays its default Art preset." The gallery's tiles are bundled thumbnails; a press saves the artwork through `/api/theme` `setArtwork`, which downloads it.
    pub(super) fn artworks_row(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dark = preview_scheme_dark(&values.string("sidebarTheme"), self.scheme_dark);
        let current = values.string(if dark { "windowArt" } else { "windowArtLight" });
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
        let mut tiles: Vec<AnyElement> = Vec::new();
        let none_selected = ghostex_settings_catalog::artwork_by_id(&current).is_none();
        tiles.push(theme_tile(
            p,
            TileSpec {
                id: "theme-artwork-none".into(),
                name: "No artwork".into(),
                label: "None".into(),
                dark,
                caption: None,
                selected: none_selected,
                cut: false,
            },
            vec![div().size_full().bg(hsla(p.raised)).into_any_element()],
            None,
            move |page, _window, cx| {
                page.theme_call(
                    "artwork-none",
                    json!({ "op": "setArtwork", "id": "none", "light": !dark }),
                    "Background not changed",
                    cx,
                );
            },
            cx,
        ));
        for artwork in ghostex_settings_catalog::ARTWORKS {
            let busy = self.prompt_gallery.busy.as_deref()
                == Some(format!("artwork-{}", artwork.id).as_str());
            let preview = self
                .artwork_thumbnail(artwork.id)
                .map(|picture| screenshot_preview(picture, busy))
                .into_iter()
                .collect();
            let label = labels
                .iter()
                .find(|(id, _)| id == artwork.id)
                .map(|(_, label)| label.clone())
                .unwrap_or_else(|| artwork.name.to_string());
            let id = artwork.id;
            tiles.push(theme_tile(
                p,
                TileSpec {
                    id: SharedString::from(format!("theme-artwork-{id}")),
                    name: artwork.name.into(),
                    label: label.into(),
                    dark,
                    caption: None,
                    selected: current == id,
                    cut: false,
                },
                preview,
                None,
                move |page, _window, cx| {
                    page.theme_call(
                        &format!("artwork-{id}"),
                        json!({ "op": "setArtwork", "id": id, "light": !dark }),
                        "Background not changed",
                        cx,
                    );
                },
                cx,
            ));
        }
        let scroll = self.prompt_gallery.artworks_scroll.clone();
        v_flex()
            .w_full()
            .min_w_0()
            .child(
                v_flex()
                    .px(px(16.0))
                    .pt(px(12.0))
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(19.0))
                            .text_color(hsla(p.foreground))
                            .child("Backgrounds"),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(18.57))
                            .text_color(hsla(p.muted))
                            .child(if dark {
                                "Any theme's artwork behind any theme, in dark mode. Each downloads the first time it shows; until then the window stays solid."
                            } else {
                                "Any theme's artwork behind any theme, in light mode. Each downloads the first time it shows; until then the window stays solid."
                            }),
                    ),
            )
            .child(
                div()
                    .id("theme-artworks-row")
                    .w_full()
                    .flex()
                    .items_start()
                    .overflow_x_scroll()
                    .track_scroll(&scroll)
                    .px(px(16.0))
                    .pt(px(6.0))
                    .pb(px(12.0))
                    .gap(px(8.0))
                    .children(tiles),
            )
            .into_any_element()
    }

    /// The applied look's line, under the themes row.
    pub(super) fn look_applied_row(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        applied_line(p, &gallery_themes(), values, &self.prompt_gallery, cx)
    }

    /// Describe a look: what an agent can do, and Ask an agent.
    pub(super) fn describe_look_row(
        &mut self,
        p: &SettingsPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        h_flex()
            .w_full()
            .gap(px(12.0))
            .px(px(16.0))
            .py(px(12.0))
            .child(settings_icon("modals/settings/sparkles.svg", 14.0, p.muted).flex_shrink_0())
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(19.0))
                            .text_color(hsla(p.foreground))
                            .child(DESCRIBE_TITLE),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(18.57))
                            .text_color(hsla(p.muted))
                            .child(DESCRIBE_COPY),
                    ),
            )
            .child(settings_sized_button(
                p,
                "theme-prompt-ask-agent",
                "Ask an agent",
                Some("modals/settings/sparkles.svg"),
                None,
                SizedButtonVariant::Secondary,
                SizedButtonSize::Sm,
                false,
                None,
                |page: &mut Self, window, cx| page.ask_agent_to_theme(STARTER_PROMPT, window, cx),
                cx,
            ))
            .into_any_element()
    }
}

/// A tile's preview: the theme's screenshot filling it.
pub(super) fn screenshot_preview(picture: Arc<Image>, busy: bool) -> AnyElement {
    div()
        .relative()
        .size_full()
        .overflow_hidden()
        .rounded(px(7.0))
        .when(busy, |this| this.opacity(0.5))
        .child(
            img(picture)
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .object_fit(ObjectFit::Cover),
        )
        .into_any_element()
}
