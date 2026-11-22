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
    AnyElement, Context, Image, ImageFormat, IntoElement, ObjectFit, ParentElement as _,
    SharedString, Styled as _, StyledImage as _, Window, div, img, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Map, Value, json};
use std::sync::Arc;
use std::time::Duration;

const DESCRIBE_TITLE: &str = "Describe a look";
const DESCRIBE_COPY: &str = "An agent sets the colours and glass from your words, like \u{201c}a calm, dark look with frosted glass\u{201d}. Your fonts stay as they are.";
/// The draft Ask an agent stages in the Help chat.
const STARTER_PROMPT: &str = "Give Ghostex a calm, dark look with soft colours and frosted glass.";
const DESIGN_AI_TITLE: &str = "Design a theme with Ask Ghostex AI";
const DESIGN_AI_COPY: &str = "Describe the artwork and mood you want in Ask Ghostex AI. It generates the image with Codex and applies it with matching colours right away.";
const DESIGN_AI_NEEDS_CODEX: &str =
    "Add a Codex account in Settings > Accounts to generate custom theme artwork.";
/// The prompt sent straight to Ask Ghostex AI when the user clicks "Design a theme with Ask Ghostex AI".
const DESIGN_THEME_PROMPT: &str = "I want to design a custom Ghostex theme with generated artwork and matching colours.\n\nBefore generating anything, ask me a couple of short questions about what I want (mood, colour palette, light or dark mode, and what the artwork should show). Keep it brief so I can answer in a line or two.\n\nOnce I reply:\n1. Generate a 16:9 landscape wallpaper artwork from my description with no text or watermarks, using Codex's built-in image_gen tool:\n   - If you are running on Codex, call your built-in image_gen tool directly (it saves a PNG under $CODEX_HOME/generated_images/...).\n   - If you are running on another agent, run Codex through Ghostex's Codex account with `xswap run --share-history -- exec --ephemeral --skip-git-repo-check --enable image_generation \"<prompt asking Codex to generate the 16:9 artwork with its built-in image_gen tool and reply with ONLY the saved PNG path>\"`.\n2. Apply the generated artwork right away with `ghostex theme background image \"<path>\"` (add `--light` for light mode; Ghostex validates and keeps its own copy).\n3. Apply matching theme colours right away with `ghostex theme set` (appearance, custom base tint, dimming/blur, sidebar and work area tints, and matching theme colour roles like accent, text, mutedText, border, divider, userMessage, chatBox). Leave my fonts unchanged unless I ask.\n4. Tell me what you applied, and offer to tweak the artwork or colours, or save the theme into \"Your themes\" with `ghostex theme save \"<name>\"`.";
/// Whether `/api/agentAccounts` reports an added, enabled local Codex account.
fn has_codex_account(state: &Value) -> bool {
    state
        .get("accounts")
        .and_then(Value::as_array)
        .is_some_and(|accounts| {
            accounts.iter().any(|account| {
                account.get("provider").and_then(Value::as_str) == Some("codex")
                    && account.get("registered").and_then(Value::as_bool) == Some(true)
                    && account.get("userDisabled").and_then(Value::as_bool) != Some(true)
                    && account
                        .get("syncedFrom")
                        .and_then(Value::as_str)
                        .is_none_or(|from| from.trim().is_empty())
                    && account.get("status").and_then(Value::as_str) != Some("unavailable")
            })
        })
}
/// The captions over the first art theme and the first agent-made look in the themes row.
const ART_CAPTION: &str = "Art";
const LOOKS_CAPTION: &str = "Agent-made";
const THEME_ENDPOINT: &str = "/api/theme";
/// `/api/theme` waits up to 15s for the app to save, then 3s for the file to show it.
const THEME_TIMEOUT: Duration = Duration::from_secs(20);

/// One theme of the row (`theme_gallery` in packages/settings-catalog/src/data/themes.rs).
#[derive(Clone, Debug)]
pub(crate) struct GalleryTheme {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) prompt: String,
    /// The word under its tile, when it is not the name's first word.
    pub(crate) label: Option<String>,
    /// `default`, `art`, `look` or `preset`.
    pub(crate) kind: String,
    /// An art theme: its own artwork is its first background.
    pub(crate) art: bool,
    pub(crate) dark: bool,
    pub(crate) changes: Map<String, Value>,
    /// A preset's other appearance, added when it follows (presets.rs).
    pub(crate) partner_changes: Map<String, Value>,
    /// The backgrounds it offers (art, live, transparent, solid) and the one it starts with.
    pub(crate) backgrounds: Vec<String>,
    pub(crate) default_background: Option<String>,
    pub(crate) background_changes: Map<String, Value>,
    pub(crate) partner_background_changes: Map<String, Value>,
}

impl GalleryTheme {
    /// What `background` changes, with the partner's when `partner` follows.
    pub(crate) fn background_patch(&self, background: &str, partner: bool) -> Map<String, Value> {
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
pub(crate) fn gallery_themes() -> Vec<GalleryTheme> {
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
pub(crate) fn preview_bytes(id: &str) -> Option<&'static [u8]> {
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
pub(crate) fn artwork_thumbnail_bytes(id: &str) -> Option<&'static [u8]> {
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
    pub(super) busy: Option<String>,
    applied: Option<(String, String)>,
    /// The Artwork strip's sideways scroll (artwork.rs).
    pub(super) artworks_scroll: gpui::ScrollHandle,
    /// The axis the strip's current swipe is locked to.
    pub(super) artworks_wheel: StripWheelLock,
    /// The appearance (dark) whose picked tile was brought into view, and the tile to centre once
    /// it is laid out.
    pub(super) artworks_revealed: Option<bool>,
    pub(super) artworks_reveal: std::rc::Rc<std::cell::Cell<Option<usize>>>,
    /// Whether this computer has an added, enabled Codex account (`None` while reading).
    pub(super) codex_account: Option<bool>,
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
pub(crate) fn applied_theme<'a>(
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
    pub(super) fn theme_call(
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
    pub(super) fn artwork_thumbnail(&mut self, id: &str) -> Option<Arc<Image>> {
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

    /// The applied look's line, under the themes row.
    pub(super) fn look_applied_row(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        applied_line(p, &gallery_themes(), values, &self.prompt_gallery, cx)
    }

    /// Checks `/api/agentAccounts` (cached read) for an added Codex account so the
    /// "Design a theme with Ask Ghostex AI" button enables when artwork generation is available.
    pub(super) fn load_codex_account(&mut self, cx: &mut Context<Self>) {
        if !self.store.read(cx).request().gxserver_rpc_available {
            self.prompt_gallery.codex_account = Some(false);
            return;
        }
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store,
            "/api/agentAccounts",
            json!({ "operation": "list", "cachedOnly": true }),
            Duration::from_secs(15),
            move |result, cx| {
                let has_codex = result.ok().is_some_and(|state| has_codex_account(&state));
                let _ = this.update(cx, |page, cx| {
                    page.prompt_gallery.codex_account = Some(has_codex);
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// Opens Ask Ghostex AI, sends the theme-design prompt, and closes Settings so the chat shows.
    fn design_theme_with_chief(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        post_store_message(
            &self.store,
            json!({ "type": "designThemeWithChief", "prompt": DESIGN_THEME_PROMPT }),
            cx,
        );
        window.remove_window();
        let store = self.store.clone();
        store.update(cx, |store, cx| store.close(cx));
    }

    /// Design a theme with Ask Ghostex AI: enabled when a Codex account is added in Ghostex;
    /// disabled with a short note when none is added yet (and disabled quietly while loading).
    pub(super) fn design_theme_with_ai_row(
        &mut self,
        p: &SettingsPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled = self.prompt_gallery.codex_account == Some(true);
        let copy = if self.prompt_gallery.codex_account == Some(false) {
            DESIGN_AI_NEEDS_CODEX
        } else {
            DESIGN_AI_COPY
        };
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
                            .child(DESIGN_AI_TITLE),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(18.57))
                            .text_color(hsla(p.muted))
                            .child(copy),
                    ),
            )
            .child(settings_sized_button(
                p,
                "theme-design-with-ai",
                "Design with Ask Ghostex AI",
                Some("modals/settings/sparkles.svg"),
                None,
                SizedButtonVariant::Secondary,
                SizedButtonSize::Sm,
                !enabled,
                None,
                |page: &mut Self, window, cx| page.design_theme_with_chief(window, cx),
                cx,
            ))
            .into_any_element()
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
