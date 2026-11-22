//! Themes > Artwork: what is painted behind the sidebar and the work area in the appearance on
//! screen. None, every theme's artwork, Your image… (a picture the user picks) and Live, in one
//! sideways strip; under it, while an artwork or Your image shows, Strength, Dimming and Blur, and
//! while Live shows, Live's gallery. The Window transparency card below (transparency.rs) is the
//! see-through window, paused while an artwork is painted.
//!
//! CDXC:Theming 2026-11-22 DECISION:
//! User, choosing proposal C ("art-in-themes") of the Themes and backdrop reorganisation page (Q4470 to Q4474): the Artwork strip stays in Themes, renamed from Backgrounds, and the Glass card becomes a slim Window transparency card (Transparency, Show behind with Fully transparent and Wallpaper as two small tiles, Strength, Blur). The duplicate Art behind the window dropdown and the theme's Background row go. One Blur slider at a time drives `windowArtBlur` under an artwork and `windowGlassBlurRadius` in Window transparency, each keeping its own value (Q4472). Dimming stays beside Strength (Q4474). Live becomes the strip's last tile ("keep Live at the end of the art options (merge the 2)", Q4473), Picture leaves the page (a saved one keeps working and Show behind says so), and Your image… before Live opens a file dialog: gxserver keeps a copy beside the downloaded artworks and it takes the same Strength, Dimming and Blur. Every key keeps working: Picture and the Live settings are untouched, and Live picked here writes the same `windowGlassSource` it always did.
//! SEE-ALSO: packages/settings-catalog/src/data/themes.rs (`YOUR_IMAGE_ART_ID`), server/src/theme/art.rs (`keep_your_image`), apps/desktop/src/app/helpers/window_art.rs.
use super::super::super::super::native_modal_kit::*;
use super::super::super::catalog::{module, settings_catalog};
use super::super::super::fields::{
    ButtonVariant, SizedButtonSize, SizedButtonVariant, SliderBinding, reset_key, settings_icon,
    settings_icon_button, settings_sized_button, slider_number_field,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::SettingsValues;
use super::colours::preview_scheme_dark;
use super::controls::indented_row;
use super::presets::{TileSpec, theme_tile};
use super::prompt_gallery::{gallery_themes, screenshot_preview};
use super::{ThemeCx, ThemeTab, gallery};
use ghostex_settings_catalog::{YOUR_IMAGE_ART_ID, artwork_by_id, your_image_key};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ObjectFit, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, StyledImage as _, Window, canvas,
    div, img, point, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Map, Value, json};
use std::path::PathBuf;

/// The art setting of one appearance.
pub(super) fn art_key(dark: bool) -> &'static str {
    if dark { "windowArt" } else { "windowArtLight" }
}

/// Whether Live shows behind the glass in this appearance: transparency on there, and Live
/// behind it.
pub(super) fn live_shows(values: &SettingsValues, dark: bool) -> bool {
    let glass = values.string("windowGlass");
    values.string("windowGlassSource") == "live" && glass != "opaque" && !(glass == "auto" && !dark)
}

/// What Artwork shows as picked in one appearance.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) enum Backdrop {
    None,
    Artwork(&'static str),
    YourImage,
    Live,
}

pub(super) fn backdrop(values: &SettingsValues, dark: bool) -> Backdrop {
    let art = values.string(art_key(dark));
    if art == YOUR_IMAGE_ART_ID {
        return Backdrop::YourImage;
    }
    if let Some(artwork) = artwork_by_id(&art) {
        return Backdrop::Artwork(artwork.id);
    }
    if live_shows(values, dark) {
        Backdrop::Live
    } else {
        Backdrop::None
    }
}

/// The name of what is painted behind a solid window in this appearance (an artwork or Your
/// image), for the Window transparency card's paused line.
pub(super) fn painted_name(values: &SettingsValues, dark: bool) -> Option<String> {
    match backdrop(values, dark) {
        Backdrop::Artwork(id) => artwork_by_id(id).map(|artwork| artwork.name.to_string()),
        Backdrop::YourImage => Some("Your image".to_string()),
        Backdrop::None | Backdrop::Live => None,
    }
}

/// The picture Your image keeps for one appearance, when there is one.
pub(super) fn your_image_path(values: &SettingsValues, dark: bool) -> Option<PathBuf> {
    let path = values.string(your_image_key(!dark));
    let path = path.trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// A picture file filling a tile's preview.
pub(super) fn file_preview(path: PathBuf, busy: bool) -> AnyElement {
    div()
        .relative()
        .size_full()
        .overflow_hidden()
        .rounded(px(7.0))
        .when(busy, |this| this.opacity(0.5))
        .child(
            img(path)
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .object_fit(ObjectFit::Cover),
        )
        .into_any_element()
}

impl ThemeTab {
    /// None: no artwork in this appearance. Live showing goes back to the see-through window
    /// (Fully transparent), so the Window transparency card below is what shows.
    pub(super) fn pick_no_artwork(&mut self, dark: bool, cx: &mut Context<Self>) {
        let values = self.store.read(cx).values();
        let mut changes = Map::new();
        changes.insert(art_key(dark).into(), json!("none"));
        if live_shows(&values, dark) {
            changes.insert("windowGlassSource".into(), json!("desktopAndWindows"));
        }
        self.theme_call(
            "artwork-none",
            json!({ "op": "set", "changes": changes }),
            "Background not changed",
            cx,
        );
    }

    /// Live: no artwork in this appearance, and Live behind the glass, turning transparency on
    /// for this appearance when it is off there.
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
        self.theme_call(
            "artwork-live",
            json!({ "op": "set", "changes": changes }),
            "Background not changed",
            cx,
        );
    }

    /// Your image: shows the kept picture, or opens the file dialog when there is none yet or it
    /// is the one showing (`change`).
    fn pick_your_image(&mut self, dark: bool, change: bool, cx: &mut Context<Self>) {
        let values = self.store.read(cx).values();
        if change || your_image_path(&values, dark).is_none() {
            self.post_picker("pickWindowArtImageFile", !dark, cx);
            return;
        }
        self.theme_call(
            "artwork-yours",
            json!({ "op": "set", "changes": { art_key(dark): YOUR_IMAGE_ART_ID } }),
            "Background not changed",
            cx,
        );
    }

    /// The Artwork strip and, under it, the controls of what it picked.
    pub(super) fn artwork_rows(
        &mut self,
        t: &ThemeCx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let p = t.p;
        let values = t.values.clone();
        let dark = preview_scheme_dark(&values.string("sidebarTheme"), self.scheme_dark);
        let picked = backdrop(&values, dark);
        let mut rows = vec![self.artwork_strip(&p, &values, dark, &picked, cx)];
        match picked {
            Backdrop::Artwork(_) | Backdrop::YourImage => {
                if picked == Backdrop::YourImage && your_image_path(&values, dark).is_none() {
                    rows.push(indented_row(self.no_picture_line(&p, dark, cx)));
                }
                rows.push(self.strength_row(t, true, window, cx));
                for (key, title, subtitle) in [
                    (
                        "windowArtDim",
                        "Dimming",
                        "How much the artwork is darkened (or lightened in light mode), so text over it stays easy to read.",
                    ),
                    (
                        "windowArtBlur",
                        "Blur",
                        "How soft the artwork looks, in points. 0 keeps it sharp.",
                    ),
                ] {
                    let upper = if key == "windowArtDim" { "DIM" } else { "BLUR" };
                    let number = |name: String| settings_catalog().number(module::SETTINGS, &name);
                    rows.push(slider_number_field(
                        self,
                        &p,
                        t.spec(key, title, subtitle).dependent(),
                        Some(reset_key::<Self>(key)),
                        SliderBinding {
                            key,
                            min: number(format!("MIN_WINDOW_ART_{upper}")),
                            max: number(format!("MAX_WINDOW_ART_{upper}")),
                            step: 1.0,
                        },
                        values.f64(key),
                        window,
                        cx,
                    ));
                }
            }
            Backdrop::Live => {
                let glass = values.string("windowGlass");
                let single = glass == "auto"
                    || values
                        .map()
                        .get("windowGlassSameForLightAndDark")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                rows.push(indented_row(self.live_gallery(
                    &p,
                    &values,
                    single,
                    t.native_picker,
                    cx,
                )));
            }
            Backdrop::None => {}
        }
        rows
    }

    /// Your image is picked but no picture is here (a theme synced from another computer).
    fn no_picture_line(
        &mut self,
        p: &SettingsPalette,
        dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        h_flex()
            .w_full()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.0))
                    .line_height(px(17.14))
                    .text_color(hsla(p.muted))
                    .child("No picture is chosen on this computer, so the window stays solid."),
            )
            .child(settings_sized_button(
                p,
                "theme-artwork-yours-choose",
                "Choose picture\u{2026}",
                None,
                None,
                SizedButtonVariant::Secondary,
                SizedButtonSize::Sm,
                false,
                None,
                move |page: &mut Self, _window, cx| page.pick_your_image(dark, true, cx),
                cx,
            ))
            .into_any_element()
    }

    /// The strip: None, the artworks, Your image… and Live, the picked one ringed.
    fn artwork_strip(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        dark: bool,
        picked: &Backdrop,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = self.prompt_gallery.busy.clone();
        let is_busy = |id: &str| busy.as_deref() == Some(id);
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
        tiles.push(theme_tile(
            p,
            TileSpec {
                id: "theme-artwork-none".into(),
                name: "No artwork".into(),
                label: "None".into(),
                dark,
                caption: None,
                selected: *picked == Backdrop::None,
                cut: false,
            },
            vec![div().size_full().bg(hsla(p.raised)).into_any_element()],
            None,
            move |page, _window, cx| page.pick_no_artwork(dark, cx),
            cx,
        ));
        for artwork in ghostex_settings_catalog::ARTWORKS {
            let preview = self
                .artwork_thumbnail(artwork.id)
                .map(|picture| {
                    screenshot_preview(picture, is_busy(&format!("artwork-{}", artwork.id)))
                })
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
                    selected: *picked == Backdrop::Artwork(id),
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
        // Your image…: the kept picture, or a quiet + until one is chosen; Change picture on hover.
        let path = your_image_path(values, dark);
        let has_picture = path.is_some();
        let yours_preview = match path {
            Some(path) => file_preview(path, is_busy("artwork-yours")),
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(hsla(p.foreground_alpha(0.04)))
                .child(settings_icon("modals/settings/plus.svg", 16.0, p.muted))
                .into_any_element(),
        };
        let change = has_picture.then(|| {
            click_shield(div().rounded(px(7.0)).bg(hsla(p.popup_background)).child(
                settings_icon_button(
                    p,
                    "theme-artwork-yours-change",
                    "modals/settings/photo.svg",
                    12.0,
                    22.0,
                    ButtonVariant::Outline,
                    Some("Choose another picture".into()),
                    false,
                    move |page: &mut Self, _window, cx| page.pick_your_image(dark, true, cx),
                    cx,
                ),
            ))
        });
        let yours_picked = *picked == Backdrop::YourImage;
        tiles.push(theme_tile(
            p,
            TileSpec {
                id: "theme-artwork-yours".into(),
                name: "Your image: any picture behind the window".into(),
                label: "Your image".into(),
                dark,
                caption: None,
                selected: yours_picked,
                cut: false,
            },
            vec![yours_preview],
            change,
            move |page, _window, cx| page.pick_your_image(dark, yours_picked, cx),
            cx,
        ));
        // Live: the poster of the style that shows in this appearance.
        let style = values.string(if dark || values.string("windowGlass") == "auto" {
            "windowGlassLiveStyleDark"
        } else {
            "windowGlassLiveStyleLight"
        });
        let poster_key = format!("{style}-{}", if dark { "dark" } else { "light" });
        let poster = match self.posters.get(&poster_key) {
            Some(poster) => Some(poster.clone()),
            None => {
                let poster = gallery::poster(if style == "video" { "ink" } else { &style }, !dark);
                if let Some(poster) = &poster {
                    self.posters.insert(poster_key, poster.clone());
                }
                poster
            }
        };
        tiles.push(theme_tile(
            p,
            TileSpec {
                id: "theme-artwork-live".into(),
                name: "Live: a calm animation in your theme colours, or your own video".into(),
                label: "Live".into(),
                dark,
                caption: None,
                selected: *picked == Backdrop::Live,
                cut: false,
            },
            poster
                .map(|poster| screenshot_preview(poster, is_busy("artwork-live")))
                .into_iter()
                .collect(),
            None,
            move |page, _window, cx| page.pick_live(dark, cx),
            cx,
        ));
        let scroll = self.prompt_gallery.artworks_scroll.clone();
        let wheel = strip_wheel_layer(&scroll, &self.prompt_gallery.artworks_wheel);
        // The picked tile is brought into view once per appearance (Your image and Live sit past
        // the 32 artworks); None needs no scrolling, and the settings arrive after the first render.
        let artworks = ghostex_settings_catalog::ARTWORKS;
        let picked_slot = match picked {
            Backdrop::None => None,
            Backdrop::Artwork(id) => artworks
                .iter()
                .position(|artwork| artwork.id == *id)
                .map(|index| index + 1),
            Backdrop::YourImage => Some(artworks.len() + 1),
            Backdrop::Live => Some(artworks.len() + 2),
        };
        if let Some(slot) = picked_slot
            && self.prompt_gallery.artworks_revealed != Some(dark)
        {
            self.prompt_gallery.artworks_revealed = Some(dark);
            self.prompt_gallery.artworks_reveal.set(Some(slot));
        }
        let reveal = self.prompt_gallery.artworks_reveal.clone();
        let measure = scroll.clone();
        let page = cx.entity_id();
        let mode = if dark { "dark" } else { "light" };
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
                            .child("Artwork"),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(18.57))
                            .text_color(hsla(p.muted))
                            .child(format!(
                                "Painted behind the sidebar and the work area in {mode} mode, with any theme. An artwork downloads the first time it shows; Your image\u{2026} uses a picture of yours. Pick None to see through the window with Window transparency below."
                            )),
                    ),
            )
            .child(
                div()
                    .relative()
                    .w_full()
                    .min_w_0()
                    .child(
                        div()
                            .id("theme-artworks-row")
                            .w_full()
                            .flex()
                            .items_start()
                            .overflow_x_scroll()
                            .restrict_scroll_to_axis()
                            .track_scroll(&scroll)
                            .px(px(16.0))
                            .pt(px(6.0))
                            .pb(px(12.0))
                            .gap(px(8.0))
                            .children(tiles),
                    )
                    .child(wheel)
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |_, _, _, cx| {
                                // The picked tile waits for its first layout, then centres.
                                if let Some(slot) = reveal.get()
                                    && let Some(tile) = measure.bounds_for_item(slot)
                                {
                                    reveal.set(None);
                                    let view = measure.bounds();
                                    let centre = tile.left() - view.left() + tile.size.width / 2.0;
                                    let target = (view.size.width / 2.0 - centre)
                                        .clamp(-measure.max_offset().x, px(0.0));
                                    measure.set_offset(point(target, measure.offset().y));
                                    cx.notify(page);
                                }
                            },
                        )
                        .absolute()
                        .size_0(),
                    ),
            )
            .into_any_element()
    }
}
