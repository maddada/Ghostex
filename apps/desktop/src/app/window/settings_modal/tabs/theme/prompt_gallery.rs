//! The "Theme it in your words" card in the Themes page's Themes group: it teaches that an agent can
//! restyle Ghostex from a plain description, with Ask an agent (a Help chat with a starter prompt
//! as its draft), and a gallery of example themes (`THEME_GALLERY` in
//! packages/settings-catalog/src/data/theme_gallery.rs), each with Apply and Copy prompt.
//!
//! CDXC:Theming 2026-11-18 DECISION:
//! User, after picking six example themes from two rounds of agent-made looks: "the final 6 left implement those please in themes page". They asked that the Interface page teach people they can prompt an agent to theme Ghostex. The card follows the approved mockup (docs/2026-11-18/theme-by-prompt/index.html): a starter prompt, then the six themes as framed pictures with their name, their prompt, Apply and Copy prompt. Apply saves through `/api/theme` `set` like `ghostex theme set`, so it is one change in the theme history; the tile's button then reads Undo until another theme is applied.
//! SEE-ALSO: server/src/theme/endpoint.rs (the change and its Undo), apps/desktop/src/app/titlebar/help_menu.rs (`ask_ghostex_help`, the chat Ask an agent opens).
use super::super::super::super::native_modal_kit::*;
use super::super::super::catalog::{module, settings_catalog};
use super::super::super::fields::{
    SizedButtonSize, SizedButtonVariant, settings_section, settings_sized_button,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::{post_store_message, store_copy_to_clipboard, store_gxserver_rpc};
use super::ThemeTab;
use super::colours::preview_scheme_dark;
use gpui::{
    AnyElement, Context, Image, ImageFormat, IntoElement, ObjectFit, ParentElement as _,
    SharedString, Styled as _, StyledImage as _, Window, div, img, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

const CARD_TITLE: &str = "Theme it in your words";
const CARD_COPY: &str = "Ask an agent to change Ghostex\u{2019}s colours, fonts and glass on this computer. Describe a mood, or copy an example below.";
/// The draft Ask an agent stages in the Help chat.
const STARTER_PROMPT: &str = "Give Ghostex a calm, dark look with soft colours and frosted glass.";
const THEME_ENDPOINT: &str = "/api/theme";
/// `/api/theme` waits up to 15s for the app to save, then 3s for the file to show it.
const THEME_TIMEOUT: Duration = Duration::from_secs(20);
/// The pictures' shape (960 x 660).
const PICTURE_ASPECT: f32 = 960.0 / 660.0;
const COLUMNS: usize = 3;
/// The font settings a gallery theme leaves alone on a computer without its font.
const FONT_KEYS: [&str; 2] = ["sessionChatFontFamily", "terminalFontFamily"];

/// One row of `THEME_GALLERY`.
#[derive(Clone, Debug)]
pub(super) struct GalleryTheme {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) prompt: String,
    pub(super) changes: Map<String, Value>,
}

/// The gallery's themes in the catalog's order.
pub(super) fn gallery_themes() -> Vec<GalleryTheme> {
    let text = |entry: &Value, key: &str| {
        entry
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
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
                    changes: entry
                        .get("changes")
                        .and_then(Value::as_object)
                        .cloned()
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
}

macro_rules! picture {
    ($name:literal) => {
        include_bytes!(concat!(
            "../../../../../../assets/modals/settings/theme-gallery/",
            $name,
            ".webp"
        ))
    };
}

/// The WebP picture of gallery theme `id`.
fn picture_bytes(id: &str) -> Option<&'static [u8]> {
    Some(match id {
        "firefly-forest" => picture!("firefly-forest"),
        "ember-forge" => picture!("ember-forge"),
        "synthwave-84" => picture!("synthwave-84"),
        "polar-night" => picture!("polar-night"),
        "inkstone" => picture!("inkstone"),
        "orchid-dusk" => picture!("orchid-dusk"),
        _ => return None,
    })
}

/// What the gallery is doing: the theme whose Apply or Undo is on its way, and the last theme
/// applied here with its change, which its tile can undo.
#[derive(Default)]
pub(super) struct PromptGallery {
    busy: Option<String>,
    applied: Option<(String, String)>,
}

/// `changes` without the fonts this computer does not have.
fn changes_for_this_computer(
    changes: &Map<String, Value>,
    installed: &HashSet<String>,
) -> Map<String, Value> {
    changes
        .iter()
        .filter(|(key, value)| {
            !FONT_KEYS.contains(&key.as_str())
                || value
                    .as_str()
                    .is_some_and(|family| installed.contains(family))
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

/// The example theme whose colours are the ones set now: every theme colour it sets still has
/// the value it gave, with how many it sets and whether Your messages is one.
fn applied_theme<'a>(
    themes: &'a [GalleryTheme],
    roles: &Value,
) -> Option<(&'a GalleryTheme, usize, bool)> {
    use ghostex_settings_catalog::theme_roles::{format_role_value, parse_role_value};
    let same = |a: &str, b: &str| match (parse_role_value(a), parse_role_value(b)) {
        (Ok(a), Ok(b)) => format_role_value(a) == format_role_value(b),
        _ => false,
    };
    themes.iter().find_map(|theme| {
        let colours: Vec<(&str, &str, &str)> = theme
            .changes
            .iter()
            .filter_map(|(key, value)| {
                let (appearance, role) = key.split_once('.')?;
                matches!(appearance, "dark" | "light").then_some((
                    appearance,
                    role,
                    value.as_str()?,
                ))
            })
            .collect();
        let all_kept = !colours.is_empty()
            && colours.iter().all(|(appearance, role, value)| {
                roles
                    .get(*appearance)
                    .and_then(|roles| roles.get(*role))
                    .and_then(Value::as_str)
                    .is_some_and(|saved| same(saved, value))
            });
        all_kept.then(|| {
            let mine = colours.iter().any(|(_, role, _)| *role == "userMessage");
            (theme, colours.len(), mine)
        })
    })
}

/// "Orchid Dusk is applied. It set 16 colours, your messages included." and Change them, which
/// opens the colours it set (CDXC:Theming 2026-11-18 in packages/settings-catalog/src/theme_roles.rs,
/// Q3981 "both").
fn applied_line(
    p: &SettingsPalette,
    themes: &[GalleryTheme],
    roles: &Value,
    cx: &mut Context<ThemeTab>,
) -> Option<AnyElement> {
    let (theme, count, mine) = applied_theme(themes, roles)?;
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

    /// Asks an agent: a Help chat opens with the starter prompt as its draft, and Settings closes
    /// so the chat shows. The window is removed here, as the shell's own close does
    /// (settings_modal/shell.rs): the store's close only tells the app the modal is gone.
    fn ask_agent_to_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        post_store_message(
            &self.store,
            json!({ "type": "askGhostexHelp", "question": STARTER_PROMPT }),
            cx,
        );
        window.remove_window();
        let store = self.store.clone();
        store.update(cx, |store, cx| store.close(cx));
    }

    /// Applies gallery theme `id` as one theme change. The themes are dark mode only, so an app
    /// showing light mode switches to Dark with it.
    fn apply_gallery_theme(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(theme) = gallery_themes().into_iter().find(|theme| theme.id == id) else {
            return;
        };
        let installed: HashSet<String> = cx.text_system().all_font_names().into_iter().collect();
        let mut changes = changes_for_this_computer(&theme.changes, &installed);
        let values = self.store.read(cx).values();
        if !preview_scheme_dark(&values.string("sidebarTheme"), self.scheme_dark) {
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
                        Err(message) => store
                            .update(cx, |store, cx| store.toast("warning", failed, &message, cx)),
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    fn gallery_picture(&mut self, id: &str) -> Option<Arc<Image>> {
        let key = format!("theme-gallery-{id}");
        if let Some(picture) = self.posters.get(&key) {
            return Some(picture.clone());
        }
        let picture = Arc::new(Image::from_bytes(
            ImageFormat::Webp,
            picture_bytes(id)?.to_vec(),
        ));
        self.posters.insert(key, picture.clone());
        Some(picture)
    }

    /// The card: what an agent can do, Ask an agent, and the example themes.
    pub(super) fn prompt_gallery_section(
        &mut self,
        p: &SettingsPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let starter = h_flex()
            .w_full()
            .gap(px(16.0))
            .px(px(16.0))
            .py(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(13.0))
                    .line_height(px(19.0))
                    .text_color(hsla(p.foreground))
                    .child(format!("\u{201c}{STARTER_PROMPT}\u{201d}")),
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
                |page: &mut Self, window, cx| page.ask_agent_to_theme(window, cx),
                cx,
            ))
            .into_any_element();
        let themes = gallery_themes();
        let mut tiles: Vec<AnyElement> = themes
            .iter()
            .map(|theme| self.gallery_tile(p, theme, cx))
            .collect();
        let mut grid_rows = Vec::new();
        while !tiles.is_empty() {
            let rest = tiles.split_off(tiles.len().min(COLUMNS));
            let mut row = std::mem::replace(&mut tiles, rest);
            while row.len() < COLUMNS {
                row.push(div().flex_1().min_w_0().into_any_element());
            }
            grid_rows.push(
                h_flex()
                    .w_full()
                    .items_stretch()
                    .gap(px(12.0))
                    .children(row),
            );
        }
        let gallery = v_flex()
            .w_full()
            .gap(px(12.0))
            .px(px(16.0))
            .pt(px(12.0))
            .pb(px(16.0))
            .child(
                h_flex()
                    .w_full()
                    .items_end()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(hsla(p.foreground))
                            .child("A few starting points"),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .line_height(px(16.0))
                            .text_color(hsla(p.muted))
                            .child("Make any of these your own"),
                    ),
            )
            .children(grid_rows)
            .into_any_element();
        let applied = applied_line(
            p,
            &themes,
            &self.store.read(cx).values().value("themeRoles"),
            cx,
        );
        let rows = if themes.is_empty() {
            vec![starter]
        } else {
            [Some(starter), applied, Some(gallery)]
                .into_iter()
                .flatten()
                .collect()
        };
        settings_section(
            p,
            CARD_TITLE,
            Some(SharedString::from(CARD_COPY)),
            None,
            rows,
        )
        .map(IntoElement::into_any_element)
    }

    /// One example theme: its framed picture, name and prompt, then Apply (Undo once applied
    /// here) and Copy prompt.
    fn gallery_tile(
        &mut self,
        p: &SettingsPalette,
        theme: &GalleryTheme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let picture = self.gallery_picture(&theme.id);
        let busy = self.prompt_gallery.busy.is_some();
        let applied_here = self
            .prompt_gallery
            .applied
            .as_ref()
            .is_some_and(|(id, _)| *id == theme.id);
        let id = theme.id.clone();
        let prompt = theme.prompt.clone();
        let apply = settings_sized_button(
            p,
            SharedString::from(format!("theme-gallery-apply-{}", theme.id)),
            if applied_here { "Undo" } else { "Apply" },
            None,
            None,
            SizedButtonVariant::Secondary,
            SizedButtonSize::Xs,
            busy,
            None,
            move |page: &mut Self, _window, cx| {
                if applied_here {
                    page.undo_gallery_theme(cx);
                } else {
                    page.apply_gallery_theme(&id, cx);
                }
            },
            cx,
        );
        let copy = settings_sized_button(
            p,
            SharedString::from(format!("theme-gallery-copy-{}", theme.id)),
            "Copy prompt",
            Some("modals/settings/copy.svg"),
            None,
            SizedButtonVariant::Outline,
            SizedButtonSize::Xs,
            false,
            None,
            move |page: &mut Self, _window, cx| {
                store_copy_to_clipboard(&page.store, prompt.clone(), cx);
            },
            cx,
        );
        v_flex()
            .flex_1()
            .min_w_0()
            .rounded(px(10.0))
            .border_1()
            .border_color(hsla(p.hairline))
            .overflow_hidden()
            .child(
                div()
                    .w_full()
                    .aspect_ratio(PICTURE_ASPECT)
                    .overflow_hidden()
                    .children(picture.map(|picture| {
                        img(picture).w_full().h_full().object_fit(ObjectFit::Cover)
                    })),
            )
            .child(
                v_flex()
                    .flex_1()
                    .gap(px(4.0))
                    .px(px(10.0))
                    .pt(px(8.0))
                    .pb(px(10.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(hsla(p.foreground))
                            .child(theme.name.clone()),
                    )
                    .child(
                        // It grows so every tile's buttons line up along the row's bottom.
                        div()
                            .flex_1()
                            .text_size(px(11.0))
                            .line_height(px(16.0))
                            .text_color(hsla(p.muted))
                            .child(theme.prompt.clone()),
                    )
                    .child(
                        h_flex()
                            .pt(px(6.0))
                            .gap(px(6.0))
                            .flex_wrap()
                            .child(apply)
                            .child(copy),
                    ),
            )
            .into_any_element()
    }
}
