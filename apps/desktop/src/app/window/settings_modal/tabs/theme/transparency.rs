//! The Transparency group of the Theme page: one Off / Dark only / Always control, Strength, Blur
//! (the points slider, or Windows' own blur choice) and Behind the glass with its pictures or Live
//! gallery; under More transparency options the picture position, Live speed and brightness, Play
//! only when plugged in, the four tints and Menu blur.
//!
//! CDXC:Theming 2026-09-26 DECISION:
//! User: rename Automatic to "Dark only", and when it is picked hide every light-mode transparency control (the light tints, the light picture and the light video); their saved values stay for when Always is picked again.
use super::super::super::super::native_modal_kit::*;
use super::super::super::catalog::{SettingOption, module, settings_catalog};
use super::super::super::fields::{
    SizedButtonSize, SizedButtonVariant, SliderBinding, SliderSaver, reset_key, segmented_field,
    select_field, setting_row, settings_sized_button, slider_number_field,
    slider_number_field_with, stock_segmented, text_field, toggle_field, toggle_field_with,
    tooltip_text,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::{SettingsStore, post_store_message};
use super::colours::{
    STRENGTH_MAX, STRENGTH_MIN, STRENGTH_STEP, strength_from_settings, strength_gaps,
    strength_patch,
};
use super::controls::{
    ICON_BEHIND_GLASS, ICON_BLUR, ICON_STRENGTH, ICON_TRANSPARENCY, indented_row,
    more_options_button, subhead,
};
use super::{MoreGroup, ThemeCx, ThemeTab, art, gallery, more_has_hit};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, InteractiveElement as _, IntoElement, ObjectFit,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _, Window, div, img, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Map, json};
use std::rc::Rc;

/// The dark-mode glass keys and their light-mode twins, which Same for light and dark folds
/// into one (CDXC:Settings 2026-10-29 DECISION, cleanup Q19, in
/// packages/settings-catalog/src/general/theme.rs).
pub(crate) const GLASS_PAIRS: [(&str, &str); 5] = [
    ("windowGlassImageDark", "windowGlassImageLight"),
    ("windowGlassVideoDark", "windowGlassVideoLight"),
    ("windowGlassLiveStyleDark", "windowGlassLiveStyleLight"),
    (
        "windowGlassSidebarOpacityDark",
        "windowGlassSidebarOpacityLight",
    ),
    (
        "windowGlassWorkAreaTintDark",
        "windowGlassWorkAreaTintLight",
    ),
];

/// `shared_settings::window_glass_same_for_light_and_dark`, which the app's glass follows: the
/// saved switch, else on unless a pair was already set apart (both values saved and different).
/// Kept in step by hand; the preview binary has no `shared_settings`.
fn same_for_light_and_dark(values: &super::super::super::store::SettingsValues) -> bool {
    let object = values.map();
    if let Some(same) = object
        .get("windowGlassSameForLightAndDark")
        .and_then(serde_json::Value::as_bool)
    {
        return same;
    }
    GLASS_PAIRS.iter().all(
        |(dark, light)| match (object.get(*dark), object.get(*light)) {
            (None, None) => true,
            (dark, light) => dark == light,
        },
    )
}

/// `GLASS_SOURCE_CARDS`: what the glass shows, as the four picture cards of step 1.
const SOURCE_CARDS: [(&str, &str, &str); 4] = [
    (
        "desktopAndWindows",
        "Desktop and windows",
        "Everything behind Ghostex, blurred.",
    ),
    ("wallpaper", "Wallpaper", "Just your desktop picture."),
    ("customImage", "Picture", "An image you choose."),
    ("live", "Live", "A calm animation, or your own video."),
];

/// The keys the Transparency card shows outside its More row.
const MAIN_KEYS: [&str; 10] = [
    "windowGlass",
    "windowGlassBlurRadius",
    "windowGlassWindowsBlur",
    "windowGlassSource",
    "windowGlassImageDark",
    "windowGlassImageLight",
    "windowGlassVideoDark",
    "windowGlassVideoLight",
    "windowGlassLiveStyleDark",
    "windowGlassLiveStyleLight",
];

/// The one Transparency control: Off, Dark only, Always (`windowGlass` opaque, auto, frosted).
fn transparency_choices() -> Vec<SettingOption> {
    [
        ("Off", "opaque"),
        ("Dark only", "auto"),
        ("Always", "frosted"),
    ]
    .iter()
    .map(|(label, value)| SettingOption {
        label: label.to_string(),
        value: value.to_string(),
    })
    .collect()
}

/// CDXC:Theming 2026-09-27 DECISION:
/// User answered "Build now" to Wallpaper, Picture and Live on Windows, so Glass shows offers them on macOS and Windows, and the Linux rollout of the same day offers them on Linux. The user's own video stays off Windows: the Windows backdrop has no video player. Supersedes the 2026-09-25 rule that kept Glass shows macOS-only.
/// `windowGlassVideoAvailable`: the user's own video plays behind the glass on macOS and Linux;
/// the Windows backdrop has no video player.
fn video_available() -> bool {
    cfg!(target_os = "macos") || cfg!(target_os = "linux")
}

/// CDXC:Theming 2026-09-25 DECISION:
/// User: "let's enable transparency on windows please also if possible. like it works on mac exactly." The glass controls show on macOS and Windows (and Linux since 2026-09-27), and on Windows turning glass on took effect at the next launch until 2026-11-02, when it became live (CDXC:Theming 2026-11-02 on `live_glass_window_creation_background` in apps/desktop/src/app/helpers/window_glass.rs).
/// `windowGlassStatusNote`: why the window stays opaque when the system's own switch blocks glass.
fn status_note(blocked: bool) -> &'static str {
    let windows = cfg!(target_os = "windows");
    match (blocked, windows) {
        (false, _) => "",
        (true, true) => {
            " Transparency effects is off in Windows Settings > Personalization > Colors, so the window stays opaque until you turn it on there."
        }
        // CDXC:Theming 2026-11-14 SEE-ALSO: `desktop_withholds_blur` in app/helpers/window_glass.rs.
        (true, false) if cfg!(target_os = "linux") => LINUX_NO_BLUR_NOTE,
        (true, false) => {
            " Reduce transparency is on in System Settings > Accessibility > Display, so the window stays opaque until you turn it off."
        }
    }
}

/// Why the window is solid on a Linux desktop that does not blur, and how to get glass.
pub(in super::super) const LINUX_NO_BLUR_NOTE: &str = " Your desktop is not blurring windows, so Ghostex keeps this window solid. On Omarchy, turn on Glassy Windows (Omarchy menu > Appearance > Windows > Window Opacity), or pick a picture or video to show behind the glass.";

/// What Blur says on Windows while Desktop and windows shows behind the glass.
pub(in super::super) const WINDOWS_BLUR_DESCRIPTION: &str = "Windows draws this blur itself and has no amount to set. Acrylic is Windows' own frosted blur. Plain blur is the same blur without Windows' grey frost, so the tints can go fully clear; it takes more work to draw. Off shows what is behind sharp and is the fastest.";

/// Whether Blur is the Windows blur choice (`windowGlassWindowsBlur`; the decision is on
/// `WINDOW_GLASS_WINDOWS_BLUR` in app/helpers/window_glass.rs): on Windows, while Desktop and
/// windows shows behind the glass.
pub(in super::super) fn windows_blur_shown(
    values: &super::super::super::store::SettingsValues,
) -> bool {
    values.string("windowGlassSource") == "desktopAndWindows"
        && !ghostex_settings_catalog::availability::row_hidden(
            ghostex_settings_catalog::Platform::current(),
            "windowGlassWindowsBlur",
            |key| Some(values.string(key)),
        )
}

/// The Windows blur choice as a segmented row; Settings > Terminal's Background blur shows it too.
pub(in super::super) fn windows_blur_field<V: super::super::super::fields::SettingsPage>(
    p: &SettingsPalette,
    spec: super::super::super::fields::RowSpec,
    values: &super::super::super::store::SettingsValues,
    on_reset: Option<super::super::super::fields::PageAction<V>>,
    on_select: impl Fn(&mut V, String, &mut Window, &mut Context<V>) + Clone + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let options = settings_catalog().options(module::SETTINGS, "WINDOW_GLASS_WINDOWS_BLUR_OPTIONS");
    let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
    let selected = values.choice("windowGlassWindowsBlur", &allowed);
    segmented_field(
        p,
        "windowGlassWindowsBlur",
        spec,
        on_reset,
        &options,
        Some(selected.as_str()),
        None,
        on_select,
        cx,
    )
}

fn number(name: &str) -> f64 {
    settings_catalog().number(module::SETTINGS, name)
}

/// `styleLabel`.
fn style_label(style: &str) -> String {
    if style == "video" {
        return "Your video".to_string();
    }
    settings_catalog()
        .options(module::SETTINGS, "WINDOW_GLASS_LIVE_STYLE_OPTIONS")
        .into_iter()
        .find(|option| option.value == style)
        .map(|option| option.label)
        .unwrap_or_else(|| style.to_string())
}

/// The last `/` segment, as the React page names a chosen file.
fn file_name(path: &str) -> String {
    path.rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn card_background(p: &SettingsPalette) -> gpui::Hsla {
    hsla(p.foreground_alpha(0.03))
}

impl ThemeTab {
    fn art_image(
        &mut self,
        key: &str,
        make: impl FnOnce() -> Option<std::sync::Arc<gpui::RenderImage>>,
    ) -> Option<std::sync::Arc<gpui::RenderImage>> {
        if let Some(image) = self.images.get(key) {
            return Some(image.clone());
        }
        let image = make()?;
        self.images.insert(key.to_string(), image.clone());
        Some(image)
    }

    fn post_picker(&mut self, kind: &str, light: bool, cx: &mut Context<Self>) {
        if kind == "pickWindowGlassVideoFile" {
            self.video_error = None;
        }
        post_store_message(
            &self.store,
            json!({ "appearance": if light { "light" } else { "dark" }, "type": kind }),
            cx,
        );
        cx.notify();
    }

    pub(super) fn transparency_section(
        &mut self,
        t: &ThemeCx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let p = t.p;
        let values = t.values.clone();
        let visible_fn = t.visible.clone();
        let visible = |key: &str| visible_fn(key);
        if !(MAIN_KEYS.iter().any(|key| visible(key))
            || more_has_hit(MoreGroup::Transparency, t.searching, &visible))
        {
            return None;
        }
        let glass = values.string("windowGlass");
        let glass_on = glass != "opaque";
        let dark_only = glass == "auto";
        // Same for light and dark: one set of picture, video, Live and tint rows (Q19). The app
        // reads the dark-mode keys for light mode while it is on (helpers/window_glass.rs).
        let same = same_for_light_and_dark(&values);
        let single = dark_only || same;
        let note = status_note(t.glass_blocked);
        let mut rows: Vec<AnyElement> = Vec::new();
        // A search hit on a glass row while transparency is Off shows the control that turns it on.
        if visible("windowGlass") || (t.searching && !glass_on) {
            let options = transparency_choices();
            rows.push(segmented_field(
                &p,
                "windowGlass",
                t.spec_plain(
                    "windowGlass",
                    "Transparency",
                    &format!(
                        "Let your desktop show softly through the window. Dark only keeps light mode solid.{note}"
                    ),
                )
                .icon(ICON_TRANSPARENCY),
                Some(reset_key::<Self>("windowGlass")),
                &options,
                Some(glass.as_str()),
                None,
                |page: &mut Self, next, _window, cx| page.save("windowGlass", json!(next), cx),
                cx,
            ));
        }
        if glass_on && visible("windowGlass") {
            let (exact, nearest) = strength_from_settings(&values);
            let saver: SliderSaver = Rc::new(
                |store: &Entity<SettingsStore>, value, _commit, cx: &mut App| {
                    store.update(cx, |store, cx| {
                        let patch = strength_patch(value, strength_gaps(&store.values()));
                        store.apply_patch(patch, "settings:control", cx)
                    });
                },
            );
            rows.push(slider_number_field_with(
                self,
                &p,
                t.spec_plain(
                    "windowGlassSidebarOpacityDark",
                    "Strength",
                    if exact.is_none() {
                        "Tuned by hand under More transparency options; moving this sets the tints from the slider again, keeping the difference between the sidebar and the work area."
                    } else if windows_blur_shown(&values)
                        && values.string("windowGlassWindowsBlur") == "acrylic"
                    {
                        "Higher shows more of what is behind the window. Acrylic keeps Windows' own grey frost even at 100; Blur set to Plain blur or Off lets it go fully clear."
                    } else {
                        "Higher shows more of what is behind the window."
                    },
                )
                .icon(ICON_STRENGTH),
                Some(reset_key::<Self>("windowGlassSidebarOpacityDark")),
                SliderBinding {
                    key: "themeTransparencyStrength",
                    min: STRENGTH_MIN,
                    max: STRENGTH_MAX,
                    step: STRENGTH_STEP,
                },
                nearest,
                saver,
                window,
                cx,
            ));
        }
        let blur_hidden = ghostex_settings_catalog::availability::row_hidden(
            ghostex_settings_catalog::Platform::current(),
            "windowGlassBlurRadius",
            |key| Some(values.string(key)),
        );
        if glass_on && visible("windowGlassBlurRadius") && !blur_hidden {
            rows.push(slider_number_field(
                self,
                &p,
                t.spec_plain(
                    "windowGlassBlurRadius",
                    "Blur",
                    if cfg!(target_os = "macos") {
                        "How soft what shows behind the window looks, in points. 0 shows it sharp."
                    } else {
                        "How soft the wallpaper, picture or video behind the window looks, in points. 0 shows it sharp. Desktop and windows uses your system's own blur."
                    },
                )
                .icon(ICON_BLUR),
                Some(reset_key::<Self>("windowGlassBlurRadius")),
                SliderBinding {
                    key: "windowGlassBlurRadius",
                    min: number("MIN_WINDOW_GLASS_BLUR_RADIUS"),
                    max: number("MAX_WINDOW_GLASS_BLUR_RADIUS"),
                    step: 1.0,
                },
                values.f64("windowGlassBlurRadius"),
                window,
                cx,
            ));
        }
        if glass_on && windows_blur_shown(&values) && visible("windowGlassWindowsBlur") {
            rows.push(windows_blur_field(
                &p,
                t.spec_plain("windowGlassWindowsBlur", "Blur", WINDOWS_BLUR_DESCRIPTION)
                    .icon(ICON_BLUR),
                &values,
                Some(reset_key::<Self>("windowGlassWindowsBlur")),
                |page: &mut Self, next, _window, cx| {
                    page.save("windowGlassWindowsBlur", json!(next), cx)
                },
                cx,
            ));
        }
        if glass_on {
            rows.extend(self.behind_glass_rows(
                t,
                single,
                (!dark_only).then_some(same),
                window,
                cx,
            ));
        }
        let shown = glass_on && self.more_shown(MoreGroup::Transparency, t.searching, &visible);
        if glass_on && !t.searching {
            let open = self.more_open[MoreGroup::Transparency.index()];
            rows.push(more_options_button(
                &p,
                "theme-more-transparency",
                "More transparency options",
                "Picture position, Live speed, sidebar and work area tints, menu blur",
                shown,
                move |page, _window, cx| page.set_more_open(MoreGroup::Transparency, !open, cx),
                cx,
            ));
        }
        if shown {
            rows.extend(self.more_transparency_rows(t, single, window, cx));
        }
        super::super::super::fields::settings_section(&p, "Transparency", None, None, rows)
            .map(IntoElement::into_any_element)
    }

    /// Behind the glass: the four picture cards, the Same for light and dark switch (`same`, absent
    /// under Dark only), then the pictures or the Live gallery for the picked one right under
    /// them. With one appearance (`single`: Dark only, or Same for light and dark on) the
    /// light-mode slots stay hidden.
    fn behind_glass_rows(
        &mut self,
        t: &ThemeCx,
        single: bool,
        same: Option<bool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let p = t.p;
        let values = t.values.clone();
        let source = values.string("windowGlassSource");
        let appearances: Vec<bool> = if single {
            vec![false]
        } else {
            vec![false, true]
        };
        let pictures_visible =
            t.visible("windowGlassImageDark") || t.visible("windowGlassImageLight");
        let live_visible = [
            "windowGlassLiveStyleDark",
            "windowGlassLiveStyleLight",
            "windowGlassVideoDark",
            "windowGlassVideoLight",
        ]
        .iter()
        .any(|key| t.visible(key));
        let mut rows: Vec<AnyElement> = Vec::new();
        if !(t.visible("windowGlassSource") || pictures_visible || live_visible) {
            return rows;
        }
        let cards = self.source_cards(&p, &source, cx);
        rows.push(setting_row(
            &p,
            "windowGlassSource",
            t.spec_plain(
                "windowGlassSource",
                "Behind the glass",
                "What the glass blurs: everything behind Ghostex, just your desktop picture, a picture you choose, or Live, a calm animation in your theme colours or your own video.",
            )
            .icon(ICON_BEHIND_GLASS)
            .wide(),
            Some(reset_key::<Self>("windowGlassSource")),
            cards,
            cx,
        ));
        if let Some(same) = same.filter(|_| t.visible("windowGlassSameForLightAndDark")) {
            rows.push(toggle_field_with(
                &p,
                "windowGlassSameForLightAndDark",
                t.spec(
                    "windowGlassSameForLightAndDark",
                    "Same for light and dark",
                    "Use the dark-mode picture, video, Live background and tints in light mode too. Turn off to pick them for light mode separately.",
                ),
                same,
                Some(reset_key::<Self>("windowGlassSameForLightAndDark")),
                |page: &mut Self, checked, _window, cx| {
                    let mut patch = Map::new();
                    patch.insert("windowGlassSameForLightAndDark".to_string(), json!(checked));
                    if !checked {
                        // Light mode starts from what it showed while the two were the same.
                        let values = page.store.read(cx).values();
                        for (dark, light) in GLASS_PAIRS {
                            patch.insert(light.to_string(), values.value(dark));
                        }
                    }
                    page.save_patch(patch, cx);
                },
                cx,
            ));
        }
        if source == "customImage" && pictures_visible {
            if t.native_picker {
                rows.push(indented_row(self.picture_pair(
                    &p,
                    &values,
                    &appearances,
                    cx,
                )));
            } else {
                for light in &appearances {
                    let key = if *light {
                        "windowGlassImageLight"
                    } else {
                        "windowGlassImageDark"
                    };
                    if !t.visible(key) {
                        continue;
                    }
                    let appearance = if *light { "light" } else { "dark" };
                    rows.push(text_field(
                        self,
                        &p,
                        key,
                        t.spec(
                            key,
                            if *light {
                                "Picture for light mode"
                            } else if single {
                                "Picture"
                            } else {
                                "Picture for dark mode"
                            },
                            &format!("The picture the glass blurs in {appearance} mode."),
                        )
                        .dependent(),
                        Some(reset_key::<Self>(key)),
                        &values.string(key),
                        Some(&format!("/Users/you/Pictures/{appearance}.jpg")),
                        Some(|text: &str| text.trim().to_string()),
                        None,
                        window,
                        cx,
                    ));
                }
            }
        }
        if source == "live" && live_visible {
            rows.push(indented_row(self.live_gallery(
                &p,
                &values,
                single,
                t.native_picker,
                cx,
            )));
        }
        rows
    }

    /// The rows under More transparency options (only while transparency is on). `single`: one
    /// appearance (Dark only, or Same for light and dark on), so the light-mode tints stay hidden.
    fn more_transparency_rows(
        &mut self,
        t: &ThemeCx,
        single: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let p = t.p;
        let values = t.values.clone();
        let source = values.string("windowGlassSource");
        let uses_picture = source != "desktopAndWindows";
        let appearances: Vec<bool> = if single {
            vec![false]
        } else {
            vec![false, true]
        };
        let live_slots: Vec<String> = appearances
            .iter()
            .map(|light| {
                values.string(if *light {
                    "windowGlassLiveStyleLight"
                } else {
                    "windowGlassLiveStyleDark"
                })
            })
            .collect();
        let live_uses_animation = live_slots.iter().any(|style| style != "video");
        let live_uses_video = live_slots.iter().any(|style| style == "video");
        // A Live animation is drawn over the window itself, so only a video has a position to pick.
        let uses_placement = uses_picture && (source != "live" || live_uses_video);
        let mut rows: Vec<AnyElement> = Vec::new();
        if uses_placement && t.visible("windowGlassImagePlacement") {
            let options = settings_catalog()
                .options(module::SETTINGS, "WINDOW_GLASS_IMAGE_PLACEMENT_OPTIONS");
            let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
            rows.push(select_field(
                self,
                &p,
                "windowGlassImagePlacement",
                t.spec(
                    "windowGlassImagePlacement",
                    "Picture position",
                    "Stays with the desktop can trail the window while you drag it.",
                ),
                Some(reset_key::<Self>("windowGlassImagePlacement")),
                &options,
                &values.choice("windowGlassImagePlacement", &allowed),
                None,
                |page: &mut Self, next, _window, cx| {
                    page.save("windowGlassImagePlacement", json!(next), cx)
                },
                window,
                cx,
            ));
        }
        if source == "live" && live_uses_animation && t.visible("windowGlassLiveSpeed") {
            rows.push(slider_number_field(
                self,
                &p,
                t.spec(
                    "windowGlassLiveSpeed",
                    "Live speed",
                    "How fast the Live animation moves. 1 is its own calm pace. Your own video plays as it is.",
                ),
                Some(reset_key::<Self>("windowGlassLiveSpeed")),
                SliderBinding {
                    key: "windowGlassLiveSpeed",
                    min: number("MIN_WINDOW_GLASS_LIVE_SPEED"),
                    max: number("MAX_WINDOW_GLASS_LIVE_SPEED"),
                    step: 0.25,
                },
                values.f64("windowGlassLiveSpeed"),
                window,
                cx,
            ));
        }
        if source == "live" && live_uses_animation && t.visible("windowGlassLiveBrightness") {
            rows.push(slider_number_field(
                self,
                &p,
                t.spec(
                    "windowGlassLiveBrightness",
                    "Live brightness",
                    "How bright the Live animation glows behind the glass. Lower keeps it a subtle glow.",
                ),
                Some(reset_key::<Self>("windowGlassLiveBrightness")),
                SliderBinding {
                    key: "windowGlassLiveBrightness",
                    min: number("MIN_WINDOW_GLASS_LIVE_BRIGHTNESS"),
                    max: number("MAX_WINDOW_GLASS_LIVE_BRIGHTNESS"),
                    step: 1.0,
                },
                values.f64("windowGlassLiveBrightness"),
                window,
                cx,
            ));
        }
        if source == "live" && t.visible("windowGlassVideoOnlyOnPower") {
            rows.push(toggle_field(
                self,
                &p,
                "windowGlassVideoOnlyOnPower",
                t.spec(
                    "windowGlassVideoOnlyOnPower",
                    "Play only when plugged in",
                    "Pause the Live animation or video while your computer runs on battery. It always pauses while Ghostex is in the background or hidden.",
                ),
                values.bool("windowGlassVideoOnlyOnPower"),
                cx,
            ));
        }
        let sidebar_range = (
            number("MIN_WINDOW_GLASS_SIDEBAR_OPACITY_PERCENT"),
            number("MAX_WINDOW_GLASS_SIDEBAR_OPACITY_PERCENT"),
        );
        let work_range = (
            number("MIN_WINDOW_GLASS_WORK_AREA_TINT_PERCENT"),
            number("MAX_WINDOW_GLASS_WORK_AREA_TINT_PERCENT"),
        );
        let tints = [
            (
                "windowGlassSidebarOpacityDark",
                "Sidebar tint in dark mode",
                "How much of the desktop the sidebar hides in dark mode. Lower shows more of your desktop through it.",
                sidebar_range,
                false,
            ),
            (
                "windowGlassWorkAreaTintDark",
                "Work area tint in dark mode",
                "How much of the desktop the work area hides in dark mode, set on its own so either area can be the darker one. Lower shows more of your desktop through it.",
                work_range,
                false,
            ),
            (
                "windowGlassSidebarOpacityLight",
                "Sidebar tint in light mode",
                "How much of the desktop the sidebar hides in light mode. Lower shows more of your desktop through it.",
                sidebar_range,
                true,
            ),
            (
                "windowGlassWorkAreaTintLight",
                "Work area tint in light mode",
                "How much of the desktop the work area hides in light mode, set on its own so either area can be the darker one. Lower shows more of your desktop through it.",
                work_range,
                true,
            ),
        ];
        let shown_tints: Vec<_> = tints
            .into_iter()
            .filter(|(key, _, _, _, light)| !(*light && single) && t.visible(key))
            .collect();
        if !shown_tints.is_empty() {
            rows.push(subhead(&p, None, "Fine-tune the tints"));
        }
        for (key, label, description, range, _) in shown_tints {
            let (label, description) = if single {
                (
                    label.replace(" in dark mode", ""),
                    description.replace(" in dark mode", ""),
                )
            } else {
                (label.to_string(), description.to_string())
            };
            rows.push(slider_number_field(
                self,
                &p,
                t.spec(key, &label, &description),
                Some(reset_key::<Self>(key)),
                SliderBinding {
                    key,
                    min: range.0,
                    max: range.1,
                    step: 1.0,
                },
                values.f64(key),
                window,
                cx,
            ));
        }
        // Windows and Linux menus use the system's own blur, which has no radius to set.
        if cfg!(target_os = "macos") && t.visible("windowGlassMenuBlurRadius") {
            rows.push(slider_number_field(
                self,
                &p,
                t.spec(
                    "windowGlassMenuBlurRadius",
                    "Menu blur",
                    "How soft what shows behind menus, panels, dialogs and tooltips looks, in points. Menus, panels and dialogs blur a quarter more than this number; tooltips use it as is. 0 shows it sharp. Applies to menus opened after the change.",
                ),
                Some(reset_key::<Self>("windowGlassMenuBlurRadius")),
                SliderBinding {
                    key: "windowGlassMenuBlurRadius",
                    min: number("MIN_WINDOW_GLASS_BLUR_RADIUS"),
                    max: number("MAX_WINDOW_GLASS_BLUR_RADIUS"),
                    step: 1.0,
                },
                values.f64("windowGlassMenuBlurRadius"),
                window,
                cx,
            ));
        }
        rows
    }

    /// Behind the glass: the four picture cards.
    fn source_cards(
        &mut self,
        p: &SettingsPalette,
        selected: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ring = p.ring;
        let cards: Vec<AnyElement> = SOURCE_CARDS
            .iter()
            .map(|(value, label, description)| {
                let is_selected = *value == selected;
                let image = self.art_image(&format!("source-{value}"), || art::source_art(value));
                let value = value.to_string();
                let key = value.clone();
                let art = div()
                    .relative()
                    .w_full()
                    .h(px(46.0))
                    .mb(px(8.0))
                    .rounded(px(7.0))
                    .overflow_hidden()
                    .when_some(image, |this, image| {
                        this.child(
                            img(image)
                                .absolute()
                                .left_0()
                                .top_0()
                                .size_full()
                                .rounded(px(7.0))
                                .object_fit(ObjectFit::Fill),
                        )
                    })
                    .when(key == "desktopAndWindows", |this| {
                        // `.is-desktop::after`: a window at 35% white, and its `18px 8px` shadow at 20%.
                        this.child(
                            div()
                                .absolute()
                                .left(gpui::relative(0.1))
                                .top(gpui::relative(0.18))
                                .w(gpui::relative(0.45))
                                .h(gpui::relative(0.6))
                                .child(
                                    div()
                                        .absolute()
                                        .left(px(18.0))
                                        .top(px(8.0))
                                        .size_full()
                                        .rounded(px(4.0))
                                        .bg(hsla(modal_rgba(0xffffff, 0.2))),
                                )
                                .child(
                                    div()
                                        .absolute()
                                        .left_0()
                                        .top_0()
                                        .size_full()
                                        .rounded(px(4.0))
                                        .bg(hsla(modal_rgba(0xffffff, 0.35))),
                                ),
                        )
                    });
                v_flex()
                    .id(SharedString::from(format!("theme-glass-source-{value}")))
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.0))
                    .p(px(if is_selected { 9.0 } else { 10.0 }))
                    .rounded(px(10.0))
                    .when(is_selected, |this| this.border_2().border_color(hsla(ring)))
                    .when(!is_selected, |this| {
                        this.border_1().border_color(hsla(p.hairline))
                    })
                    .bg(card_background(p))
                    .on_click(cx.listener(move |page, _: &ClickEvent, _window, cx| {
                        page.save("windowGlassSource", json!(value.clone()), cx);
                    }))
                    .child(art)
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(20.0))
                            .text_color(hsla(p.foreground))
                            .child(*label),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .line_height(px(14.85))
                            .text_color(hsla(p.muted))
                            .child(*description),
                    )
                    .into_any_element()
            })
            .collect();
        h_flex()
            .w_full()
            .items_stretch()
            .gap(px(10.0))
            .children(cards)
            .into_any_element()
    }

    /// The dark and light pictures side by side, under the Picture card.
    fn picture_pair(
        &mut self,
        p: &SettingsPalette,
        values: &super::super::super::store::SettingsValues,
        appearances: &[bool],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let slots: Vec<AnyElement> = appearances
            .iter()
            .map(|light| {
                let light = *light;
                let key = if light {
                    "windowGlassImageLight"
                } else {
                    "windowGlassImageDark"
                };
                let path = values.string(key);
                let has_path = !path.is_empty();
                let thumb = if has_path {
                    let image = self
                        .art_image(if light { "thumb-light" } else { "thumb-dark" }, || {
                            art::picture_thumb(light)
                        });
                    div()
                        .w_full()
                        .h(px(72.0))
                        .mb(px(4.0))
                        .rounded(px(MODAL_RADIUS_CONTROL))
                        .overflow_hidden()
                        .when_some(image, |this, image| {
                            this.child(
                                img(image)
                                    .size_full()
                                    .rounded(px(MODAL_RADIUS_CONTROL))
                                    .object_fit(ObjectFit::Fill),
                            )
                        })
                        .into_any_element()
                } else {
                    let stripes = self.art_image(
                        if p.light {
                            "thumb-empty-light"
                        } else {
                            "thumb-empty-dark"
                        },
                        || art::empty_thumb_stripes(p.foreground),
                    );
                    div()
                        .relative()
                        .w_full()
                        .h(px(72.0))
                        .mb(px(4.0))
                        .rounded(px(MODAL_RADIUS_CONTROL))
                        .border_1()
                        .border_dashed()
                        .border_color(hsla(p.hairline))
                        .overflow_hidden()
                        .flex()
                        .items_center()
                        .justify_center()
                        .when_some(stripes, |this, image| {
                            this.child(
                                img(image)
                                    .absolute()
                                    .left_0()
                                    .top_0()
                                    .size_full()
                                    .object_fit(ObjectFit::Cover),
                            )
                        })
                        .child(
                            div()
                                .text_size(px(11.0))
                                .line_height(px(15.71))
                                .text_color(hsla(p.muted))
                                .child("No picture yet"),
                        )
                        .into_any_element()
                };
                let file = div()
                    .id(SharedString::from(format!(
                        "theme-glass-picture-file-{key}"
                    )))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(11.0))
                    .line_height(px(15.71))
                    .text_color(hsla(p.muted))
                    .when(has_path, |this| this.tooltip(tooltip_text(path.clone())))
                    .child(if has_path {
                        file_name(&path)
                    } else {
                        "Shows the live blur".to_string()
                    });
                let mut actions = vec![settings_sized_button(
                    p,
                    SharedString::from(format!("theme-glass-picture-choose-{key}")),
                    if has_path { "Change…" } else { "Choose…" },
                    None,
                    None,
                    SizedButtonVariant::Secondary,
                    SizedButtonSize::Sm,
                    false,
                    None,
                    move |page: &mut Self, _window, cx| {
                        page.post_picker("pickWindowGlassImageFile", light, cx)
                    },
                    cx,
                )];
                if has_path {
                    actions.push(settings_sized_button(
                        p,
                        SharedString::from(format!("theme-glass-picture-clear-{key}")),
                        "Clear",
                        None,
                        None,
                        SizedButtonVariant::Outline,
                        SizedButtonSize::Sm,
                        false,
                        None,
                        move |page: &mut Self, _window, cx| page.save(key, json!(""), cx),
                        cx,
                    ));
                }
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.0))
                    .p(px(10.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .bg(card_background(p))
                    .child(thumb)
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(18.57))
                            .text_color(hsla(p.foreground))
                            .child(if light { "Light mode" } else { "Dark mode" }),
                    )
                    .child(file)
                    .child(h_flex().mt(px(6.0)).gap(px(6.0)).children(actions))
                    .into_any_element()
            })
            .collect();
        h_flex()
            .w_full()
            .items_start()
            .gap(px(12.0))
            .children(slots)
            .into_any_element()
    }

    /// `GlassLiveGallery`: the eight animations and "Your video", one pick per appearance.
    ///
    /// CDXC:Theming 2026-09-26 DECISION:
    /// User: "let's hide videos and merge videos with live / make videos just take from custom video user picks or the animations we did". Live lists the eight animations and then "Your video", a file the user picks for each mode; picking the card with no file yet opens the file dialog.
    fn live_gallery(
        &mut self,
        p: &SettingsPalette,
        values: &super::super::super::store::SettingsValues,
        dark_only: bool,
        can_choose_video: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dark_style = values.string("windowGlassLiveStyleDark");
        let light_style = values.string("windowGlassLiveStyleLight");
        let light = !dark_only && self.live_editing_light;
        let selected = if light {
            light_style.clone()
        } else {
            dark_style.clone()
        };
        let video_key = if light {
            "windowGlassVideoLight"
        } else {
            "windowGlassVideoDark"
        };
        let style_key = if light {
            "windowGlassLiveStyleLight"
        } else {
            "windowGlassLiveStyleDark"
        };
        let video = values.string(video_key);
        let error = self
            .video_error
            .as_ref()
            .filter(|(error_light, _)| *error_light == light)
            .map(|(_, message)| message.clone());
        let head = h_flex()
            .w_full()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(if dark_only {
                div()
                    .text_size(px(13.0))
                    .line_height(px(18.57))
                    .text_color(hsla(p.foreground))
                    .child(format!("Dark mode: {}", style_label(&dark_style)))
                    .into_any_element()
            } else {
                stock_segmented(
                    p,
                    "glass-live-appearance",
                    &[
                        (
                            "dark".to_string(),
                            format!("Dark mode · {}", style_label(&dark_style)),
                        ),
                        (
                            "light".to_string(),
                            format!("Light mode · {}", style_label(&light_style)),
                        ),
                    ],
                    Some(if light { "light" } else { "dark" }),
                    false,
                    false,
                    |page: &mut Self, value, _window, cx| {
                        page.live_editing_light = value == "light";
                        cx.notify();
                    },
                    cx,
                )
            })
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(17.14))
                    .text_color(hsla(p.muted))
                    .child("Animations are drawn live in your theme colours."),
            );
        let ring = p.ring;
        let hover_border = p.foreground_alpha(0.22);
        let mut cards: Vec<AnyElement> = settings_catalog()
            .options(module::SETTINGS, "WINDOW_GLASS_LIVE_STYLE_OPTIONS")
            .into_iter()
            .map(|option| {
                let poster_key =
                    format!("{}-{}", option.value, if light { "light" } else { "dark" });
                let poster = match self.posters.get(&poster_key) {
                    Some(poster) => Some(poster.clone()),
                    None => {
                        let poster = gallery::poster(&option.value, light);
                        if let Some(poster) = &poster {
                            self.posters.insert(poster_key, poster.clone());
                        }
                        poster
                    }
                };
                let is_selected = selected == option.value;
                let value = option.value.clone();
                live_card(
                    p,
                    SharedString::from(format!("glass-live-{}", option.value)),
                    option.label.clone(),
                    poster.map(|poster| {
                        img(poster)
                            .w_full()
                            .h_full()
                            .object_fit(ObjectFit::Cover)
                            .into_any_element()
                    }),
                    is_selected,
                    ring,
                    hover_border,
                    move |page: &mut Self, cx| page.save(style_key, json!(value.clone()), cx),
                    cx,
                )
            })
            .collect();
        if video_available() {
            let has_video = !video.is_empty();
            cards.push(live_card(
                p,
                "glass-live-video".into(),
                "Your video".to_string(),
                Some(video_poster(p)),
                selected == "video",
                ring,
                hover_border,
                move |page: &mut Self, cx| {
                    // `pickVideo`: pick the card, and open the dialog while no file is chosen yet.
                    page.save(style_key, json!("video"), cx);
                    if !has_video && can_choose_video {
                        page.post_picker("pickWindowGlassVideoFile", light, cx);
                    }
                },
                cx,
            ));
        }
        let mut grid_rows: Vec<AnyElement> = Vec::new();
        let mut cards = cards.into_iter().peekable();
        while cards.peek().is_some() {
            let mut row: Vec<AnyElement> = cards.by_ref().take(3).collect();
            while row.len() < 3 {
                row.push(div().flex_1().min_w_0().into_any_element());
            }
            grid_rows.push(
                h_flex()
                    .w_full()
                    .gap(px(8.0))
                    .children(row)
                    .into_any_element(),
            );
        }
        let mut gallery = v_flex()
            .w_full()
            .gap(px(12.0))
            .pt(px(4.0))
            .pb(px(8.0))
            .child(head)
            .child(v_flex().w_full().gap(px(8.0)).children(grid_rows));
        if video_available() && selected == "video" {
            gallery = gallery.child(self.video_slot(p, light, &video, error, can_choose_video, cx));
        }
        gallery.into_any_element()
    }

    /// The chosen video's file with Change… and Clear, or why the last pick was refused.
    fn video_slot(
        &mut self,
        p: &SettingsPalette,
        light: bool,
        video: &str,
        error: Option<String>,
        can_choose: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = if light {
            "windowGlassVideoLight"
        } else {
            "windowGlassVideoDark"
        };
        let has_video = !video.is_empty();
        let actions: AnyElement = if can_choose {
            let mut buttons = vec![settings_sized_button(
                p,
                "glass-live-video-choose",
                if has_video {
                    "Change…"
                } else {
                    "Choose file…"
                },
                None,
                None,
                SizedButtonVariant::Secondary,
                SizedButtonSize::Sm,
                false,
                None,
                move |page: &mut Self, _window, cx| {
                    page.post_picker("pickWindowGlassVideoFile", light, cx)
                },
                cx,
            )];
            if has_video {
                buttons.push(settings_sized_button(
                    p,
                    "glass-live-video-clear",
                    "Clear",
                    None,
                    None,
                    SizedButtonVariant::Outline,
                    SizedButtonSize::Sm,
                    false,
                    None,
                    move |page: &mut Self, _window, cx| page.save(key, json!(""), cx),
                    cx,
                ));
            }
            h_flex().gap(px(6.0)).children(buttons).into_any_element()
        } else {
            div()
                .text_size(px(12.0))
                .line_height(px(17.14))
                .text_color(hsla(p.muted))
                .child("Videos are chosen in the Ghostex desktop app.")
                .into_any_element()
        };
        h_flex()
            .w_full()
            .flex_wrap()
            .items_center()
            .gap_x(px(12.0))
            .gap_y(px(8.0))
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(hsla(p.hairline))
            .child(
                div()
                    .id("glass-live-video-file")
                    .flex_1()
                    .min_w(px(192.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(13.0))
                    .line_height(px(18.57))
                    .text_color(hsla(p.foreground))
                    .when(has_video, |this| {
                        this.tooltip(tooltip_text(video.to_string()))
                    })
                    .child(if has_video {
                        file_name(video)
                    } else {
                        "No video chosen yet, so the glass shows the live blur.".to_string()
                    }),
            )
            .child(actions)
            .children(error.map(|error| {
                div()
                    .w_full()
                    .text_size(px(12.0))
                    .line_height(px(17.14))
                    .text_color(hsla(p.destructive))
                    .child(error)
            }))
            .into_any_element()
    }
}

/// One card of the Live grid: the 16:10 poster and the style's name, ringed when picked.
#[allow(clippy::too_many_arguments)]
fn live_card(
    p: &SettingsPalette,
    id: SharedString,
    label: String,
    poster: Option<AnyElement>,
    selected: bool,
    ring: gpui::Rgba,
    hover_border: gpui::Rgba,
    on_pick: impl Fn(&mut ThemeTab, &mut Context<ThemeTab>) + 'static,
    cx: &mut Context<ThemeTab>,
) -> AnyElement {
    let card = v_flex()
        .id(id)
        .w_full()
        .overflow_hidden()
        .rounded(px(10.0))
        .border_1()
        .border_color(hsla(if selected { ring } else { p.hairline }))
        .when(!selected, |this| {
            this.hover(move |this| this.border_color(hsla(hover_border)))
        })
        .on_click(cx.listener(move |page, _: &ClickEvent, _window, cx| on_pick(page, cx)))
        .child(
            div()
                .w_full()
                .aspect_ratio(16.0 / 10.0)
                .overflow_hidden()
                .children(poster),
        )
        .child(
            div()
                .pt(px(6.0))
                .px(px(8.0))
                .pb(px(7.0))
                .text_size(px(12.0))
                .line_height(px(17.14))
                .text_color(hsla(p.foreground))
                .child(label),
        );
    // `box-shadow: 0 0 0 2px <ring 35%>` outside the card, drawn as a ring so it does not also
    // fill under the card's transparent name strip.
    div()
        .relative()
        .flex_1()
        .min_w_0()
        .when(selected, |this| {
            this.child(
                div()
                    .absolute()
                    .left(px(-2.0))
                    .top(px(-2.0))
                    .right(px(-2.0))
                    .bottom(px(-2.0))
                    .rounded(px(12.0))
                    .border_2()
                    .border_color(hsla(css_fade(ring, 0.35))),
            )
        })
        .child(card)
        .into_any_element()
}

/// "Your video": a play glyph on a quiet wash instead of a rendered poster.
fn video_poster(p: &SettingsPalette) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::linear_gradient(
            150.0,
            gpui::linear_color_stop(hsla(p.foreground_alpha(0.10)), 0.0),
            gpui::linear_color_stop(hsla(p.foreground_alpha(0.03)), 1.0),
        ))
        // `.glass-live-video-glyph`: a CSS border triangle, 0.8rem wide and 1rem tall.
        .child(
            gpui::svg()
                .path("modals/settings/glass-live-play.svg")
                .ml(px(3.2))
                .w(px(12.8))
                .h(px(16.0))
                .text_color(hsla(p.foreground_alpha(0.55))),
        )
        .into_any_element()
}
