//! The agent-made looks of the Themes page's Themes group and the way to ask an agent for one. The
//! six example themes (`THEME_GALLERY` in packages/settings-catalog/src/data/theme_gallery.rs) lead
//! the themes row as tiles of the presets' own kind (`theme_tile` in presets.rs): a press applies
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
/// The pictures' shape (960 x 660), and how a look's tile draws one: the window inside it spans
/// about 7% to 93% across and 10% to 88% down.
const PICTURE_ASPECT: f32 = 960.0 / 660.0;
const PICTURE_WIDTH: f32 = 92.0;
const PICTURE_LEFT: f32 = -6.7;
const PICTURE_TOP: f32 = -7.5;
/// The caption over the first look in the themes row.
const LOOKS_CAPTION: &str = "Agent-made";
const THEME_ENDPOINT: &str = "/api/theme";
/// `/api/theme` waits up to 15s for the app to save, then 3s for the file to show it.
const THEME_TIMEOUT: Duration = Duration::from_secs(20);

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

/// "Orchid Dusk is applied. It set 16 colours, your messages included.", Undo while it is the
/// look last applied here, and Change them, which opens the colours it set (CDXC:Theming 2026-11-18
/// in packages/settings-catalog/src/theme_roles.rs, Q3981 "both").
fn applied_line(
    p: &SettingsPalette,
    themes: &[GalleryTheme],
    roles: &Value,
    gallery: &PromptGallery,
    cx: &mut Context<ThemeTab>,
) -> Option<AnyElement> {
    let (theme, count, mine) = applied_theme(themes, roles)?;
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

    /// Applies gallery theme `id` as one theme change. The themes are dark mode only, so an app
    /// showing light mode switches to Dark with it.
    fn apply_gallery_theme(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(theme) = gallery_themes().into_iter().find(|theme| theme.id == id) else {
            return;
        };
        // No look names a font (CDXC:Theming 2026-11-21 in theme_gallery.rs): the user's fonts stay.
        let mut changes = theme.changes;
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

    /// The looks' tiles for the themes row, each with whether it is the applied one. `cut` is the
    /// row's bits of the tiles an edge cuts; the looks are its first tiles.
    pub(super) fn look_tiles(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        cut: u64,
        cx: &mut Context<Self>,
    ) -> Vec<(bool, AnyElement)> {
        let themes = gallery_themes();
        let roles = values.value("themeRoles");
        let applied = applied_theme(&themes, &roles).map(|(theme, _, _)| theme.id.clone());
        themes
            .iter()
            .enumerate()
            .map(|(index, theme)| {
                let selected = applied.as_deref() == Some(theme.id.as_str());
                let caption = (index == 0).then(|| LOOKS_CAPTION.to_string());
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
            .map(|picture| {
                // The picture frames the window in a margin; it is drawn larger so the window
                // alone fills the preview.
                div()
                    .relative()
                    .size_full()
                    .overflow_hidden()
                    .rounded(px(7.0))
                    .when(busy, |this| this.opacity(0.5))
                    .child(
                        img(picture)
                            .absolute()
                            .left(px(PICTURE_LEFT))
                            .top(px(PICTURE_TOP))
                            .w(px(PICTURE_WIDTH))
                            .h(px(PICTURE_WIDTH / PICTURE_ASPECT))
                            .object_fit(ObjectFit::Fill),
                    )
                    .into_any_element()
            })
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
                    .name
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_string()
                    .into(),
                dark: true,
                caption,
                selected,
                cut,
            },
            preview,
            Some(own),
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
        applied_line(
            p,
            &gallery_themes(),
            &values.value("themeRoles"),
            &self.prompt_gallery,
            cx,
        )
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
