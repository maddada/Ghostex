//! Every theme Settings > Themes and `ghostex theme` offer, as one list: Default, the art themes,
//! the agent-made looks and the named presets. Each theme carries the changes it makes (as
//! `ghostex theme set` takes them) and, apart from them, what each of its backgrounds changes:
//! Art (its artwork behind an opaque window), Live (its animation behind the glass, where it has
//! one), Transparent (the desktop through the glass) and Solid (no background).
//!
//! CDXC:Theming 2026-11-21 DECISION:
//! User: "i somehow want to make all the themes have a bg behind them that can be disabled or user can switch to transparent, so we need like presets for the themes so after i pick a theme i can switch the presets", "i wanna keep #2 and #3 themes but we must have image generated to be behind each one as 1 of the presets". Every theme has the same background choices, picked after the theme (Settings > Themes' Background row, `ghostex theme apply <theme> --background art|live|transparent|solid`): Art when its artwork is bundled (`ARTWORKS`), Live when it has an animation (Default and the agent-made looks), Transparent and Solid always. Applying a theme picks its own default: Art for an art theme, Live for Default and the agent-made looks (their look until now), and for a named preset the background stays as it is, as presets always left it, so nobody's window changes behind their back.
//! SEE-ALSO: theme_gallery.rs (the looks), theme_presets.rs and preset_roles.rs (the presets), apps/desktop/src/app/helpers/window_art.rs (Art), server/src/theme/gallery.rs (`apply`).

use super::preset_roles::PRESET_ROLES;
use super::theme_gallery::THEME_GALLERY;
use super::theme_presets::THEME_PRESETS;
use crate::json::{Json, J};
use crate::theme_roles::THEME_ROLES;
use crate::Platform;

/// The clean fills every theme takes for the colours it does not set itself (`fill_roles`): faint
/// borders and dividers, opaque card, control, menu and chat box fills a small step off the window
/// colour, and quiet hover and selection washes (PR 757's clean look).
const CLEAN_THEME_FILLS: &[(&str, &[(&str, &str)])] = &[
    (
        "dark",
        &[
            ("border", "8%"),
            ("divider", "6%"),
            ("userMessage", "base+6%"),
            ("chatBox", "base+3%"),
            ("surface", "base+3%"),
            ("control", "base+5%"),
            ("menu", "base+4%"),
            ("selection", "7%"),
            ("hover", "4%"),
        ],
    ),
    (
        "light",
        &[
            ("border", "9%"),
            ("divider", "7%"),
            ("userMessage", "base+4%"),
            ("chatBox", "base-60%"),
            ("surface", "base-60%"),
            ("control", "base+3%"),
            ("menu", "base-60%"),
            ("selection", "6%"),
            ("hover", "4%"),
        ],
    ),
];

/// The id and name of the theme that puts Ghostex's shipped look back.
pub const DEFAULT_LOOK_ID: &str = "default";
pub const DEFAULT_LOOK_NAME: &str = "Default";

/// One theme artwork: what Art paints behind the window, a theme's own or any theme's when it is
/// picked as a background (Settings > Themes' Backgrounds, `ghostex theme background set`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Artwork {
    pub id: &'static str,
    pub name: &'static str,
    /// The WebP's SHA-256 (lowercase hex) and size: gxserver downloads `<id>-<first 8 of the
    /// hash>.webp` from [`ARTWORK_RELEASE`] and keeps it only when both match.
    pub sha256: &'static str,
    pub size_bytes: u64,
}

/// Where the artworks are hosted: the components repository's `theme-art` release, the public
/// repository the app's other on-demand downloads come from (tooling/release-gpui/components-repo.mjs).
pub const ARTWORK_RELEASE: &str =
    "https://github.com/maddada/ghostex-components/releases/download/theme-art";

/// The artwork's file name in [`ARTWORK_RELEASE`] and in the download cache.
pub fn artwork_file_name(artwork: &Artwork) -> String {
    format!("{}-{}.webp", artwork.id, &artwork.sha256[..8])
}

/// The artwork `id`.
pub fn artwork_by_id(id: &str) -> Option<&'static Artwork> {
    ARTWORKS.iter().find(|artwork| artwork.id == id)
}

/// `windowArt` / `windowArtLight` value that paints the user's own picture (`windowArtImage` /
/// `windowArtImageLight`, a copy gxserver keeps beside the downloaded artworks) instead of one of
/// [`ARTWORKS`].
///
/// CDXC:Theming 2026-11-22 DECISION:
/// User (Q4473 note on the Themes reorganisation): "also add custom image picking at the end of the artworks so the user can add any image". Artwork ends with Your image…, which opens a file dialog; gxserver copies the picture into its `theme-art/yours/` folder (server/src/theme/art.rs) and the window paints it like any artwork, with the same Strength, Dimming and Blur. The picture's path is kept per computer (`windowArtImage`, `windowArtImageLight`, left out of Settings Sync), so a synced `yourImage` on a computer without the picture leaves the window solid, as an artwork that can't be downloaded does.
pub const YOUR_IMAGE_ART_ID: &str = "yourImage";

/// The per-computer picture key Your image reads in dark mode, or light mode with `light`.
pub fn your_image_key(light: bool) -> &'static str {
    if light {
        "windowArtImageLight"
    } else {
        "windowArtImage"
    }
}

/// Whether `id` (a `windowArt` value) paints something: one of [`ARTWORKS`] or Your image.
pub fn art_id_paints(id: &str) -> bool {
    id == YOUR_IMAGE_ART_ID || artwork_by_id(id).is_some()
}

/// CDXC:Theming 2026-11-21 DECISION:
/// User: "i wanna reduce the bundle size not increase it": no theme artwork ships inside the app. Each is downloaded the first time it is shown, from the components repository the app's other on-demand parts come from, checked against the hash and size listed here (they are built into the app, so a changed file is refused), and kept on disk (gxserver's component store, `theme-art/`). Only small thumbnails are bundled. A theme whose artwork is not here yet applies its colours at once and the artwork fades in when it arrives; offline, the window stays solid in the theme's colour. Supersedes the six artworks bundled on 2026-11-21 (e05a071ea).
/// SEE-ALSO: server/src/theme/art.rs (the download and cache), apps/desktop/src/app/helpers/window_art.rs.
pub const ARTWORKS: &[Artwork] = &[
    Artwork {
        id: "moonwater",
        name: "Moonwater",
        sha256: "6d080bb4daa871d95cc0836de35d26adfbe72ed418eff348740b2d3868a71798",
        size_bytes: 350522,
    },
    Artwork {
        id: "polar-night",
        name: "Polar Night",
        sha256: "a7184e97de83d179f177f9235edceb50f3fa3cdd515c1244e8ffde8d81589c36",
        size_bytes: 113042,
    },
    Artwork {
        id: "glasshouse",
        name: "Glasshouse",
        sha256: "cc44012979bc456b13d845bcd75a5c8889a9126a2819cfee9429921aaa4b50ca",
        size_bytes: 499062,
    },
    Artwork {
        id: "red-earth",
        name: "Red Earth",
        sha256: "41da3d9c551c8083d9d707d1ec0b851efc46856ccfe2b133194bae0e8179890d",
        size_bytes: 178612,
    },
    Artwork {
        id: "deep-current",
        name: "Deep Current",
        sha256: "4060531dad7f13bb48cb7d73d95bbc60ed5f24ca69da3ad3958daa50d965703e",
        size_bytes: 294282,
    },
    Artwork {
        id: "night-garden",
        name: "Night Garden",
        sha256: "29317325308c19394aa7b56d053fcc60f593572c2dab81f62deb2e64d825ba7f",
        size_bytes: 282744,
    },
    Artwork {
        id: "nocturne-canyon",
        name: "Nocturne Canyon",
        sha256: "87f5accc7e668b918a68d67ef21cbcfe623088184e9c0940b35b0ee8062c5863",
        size_bytes: 189330,
    },
    Artwork {
        id: "inkstone",
        name: "Inkstone",
        sha256: "53fbe6af737e817d671f8654aea9ca73393776f57949e7dd7a6ca3076cef4821",
        size_bytes: 189638,
    },
    Artwork {
        id: "firefly-forest",
        name: "Firefly Forest",
        sha256: "9c0d8ff86fa22ee9dcdd3153ad4246231953ac94fee363c4f58ddf87cac3e4fc",
        size_bytes: 168822,
    },
    Artwork {
        id: "ember-forge",
        name: "Ember Forge",
        sha256: "403ad58947132977ddc15e530afef0f976e49c8272b4e6cc563ab6973b2dbf40",
        size_bytes: 93516,
    },
    Artwork {
        id: "synthwave-84",
        name: "Synthwave '84",
        sha256: "6a0c9321e30d0d1e6e7ad6783a2809b63c4755910a98e887dc98ea4a3e6960a1",
        size_bytes: 173314,
    },
    Artwork {
        id: "orchid-dusk",
        name: "Orchid Dusk",
        sha256: "c26bc583df15d8cf4e2fbd81875697cbb467da0f110a8c80bc405c92f98f4773",
        size_bytes: 106318,
    },
    Artwork {
        id: "mocha",
        name: "Catppuccin Mocha",
        sha256: "8fa2dbeff395562f8a4bd10c0b2c0e2b09fa32747ad3253a90457e4b1af3d2ab",
        size_bytes: 230176,
    },
    Artwork {
        id: "macchiato",
        name: "Catppuccin Macchiato",
        sha256: "fe4a4a57fc2a97f8eb5ea5299665a6864802f62885b159d59ef31126346d506d",
        size_bytes: 175542,
    },
    Artwork {
        id: "frappe",
        name: "Catppuccin Frappé",
        sha256: "b05ee8d6a57120310caefc7f579a5589cbd89c596e832ae02ce48dbec97ae025",
        size_bytes: 134738,
    },
    Artwork {
        id: "latte",
        name: "Catppuccin Latte",
        sha256: "e6dc6509f235c677ecd98e57347afedb29a31040f79058030f3552a850344dc0",
        size_bytes: 321008,
    },
    Artwork {
        id: "dracula",
        name: "Dracula",
        sha256: "02d1becadcb0326f62110da6f2d897786818c12e487bb30e8576146f1ce6c890",
        size_bytes: 210752,
    },
    Artwork {
        id: "alucard",
        name: "Alucard",
        sha256: "ff50ec477d2ce33f987c2f250886dfb29cb6e8a2ea7c286dd473c28556660345",
        size_bytes: 289726,
    },
    Artwork {
        id: "sol-dark",
        name: "Solarized Dark",
        sha256: "c423d3aa6f46c71ade02c85c0cd219d2470bdb33693a4135aabe81528625cf60",
        size_bytes: 152798,
    },
    Artwork {
        id: "sol-light",
        name: "Solarized Light",
        sha256: "94e4ee34ea46b4258bff50cfd4b1951bf21995737f79688c6e439a4e47528b5b",
        size_bytes: 220978,
    },
    Artwork {
        id: "tn-night",
        name: "Tokyo Night",
        sha256: "038cc32c84a2ca6d22a4f78d66d7b1ce9fac884a48c6ed47b2c9dc97553ec366",
        size_bytes: 161658,
    },
    Artwork {
        id: "tn-storm",
        name: "Tokyo Night Storm",
        sha256: "63edf6d4f3674db55fbf1be565dbe33a4cc34eafc2da6e1cf2df6e1e803e8371",
        size_bytes: 154126,
    },
    Artwork {
        id: "tn-moon",
        name: "Tokyo Night Moon",
        sha256: "9f9b2dbe1162ddbc4c47c11f9f802bced7154cfeb400f93a93ef35ef49ffc4aa",
        size_bytes: 216206,
    },
    Artwork {
        id: "tn-day",
        name: "Tokyo Night Day",
        sha256: "74964e9fb264d57a0afbe21f35d29d56e61f71817696d60a655b987fa9c22602",
        size_bytes: 278828,
    },
    Artwork {
        id: "nord",
        name: "Nord",
        sha256: "65f62f9ae1b4d9d648dfd7432788f50570e7cbc03d0490348631b38a3a1ef504",
        size_bytes: 133470,
    },
    Artwork {
        id: "kg-wave",
        name: "Kanagawa Wave",
        sha256: "407c43a0d1918969002cea56613b7c0b1acad78e0da921e81c959df48c50b61b",
        size_bytes: 175160,
    },
    Artwork {
        id: "kg-dragon",
        name: "Kanagawa Dragon",
        sha256: "04d8907b8ebdab5fdb57123470d04e06d4afa8e90baef2d16f175e5410c37e5c",
        size_bytes: 160706,
    },
    Artwork {
        id: "kg-lotus",
        name: "Kanagawa Lotus",
        sha256: "cdc20d9f85a7e656338e2afb0afe4efffa2b24771fb1fb44c63634749ebb8f71",
        size_bytes: 178622,
    },
    Artwork {
        id: "ef-dark",
        name: "Everforest Dark",
        sha256: "85bbe256514e94adca1c4216f408fc0781455ff09ef961d79e4ecee79da6e4a0",
        size_bytes: 167934,
    },
    Artwork {
        id: "ef-light",
        name: "Everforest Light",
        sha256: "92eb5f60fa8a0df92096ce7c47f12e0231102e5c4e80c6699f7b4267175c9de6",
        size_bytes: 273350,
    },
    Artwork {
        id: "gm-dark",
        name: "Gruvbox Material Dark",
        sha256: "5895ad6973964a1262b97cc721e3ca4c311683ee39b44607d096f1c683d6648e",
        size_bytes: 250932,
    },
    Artwork {
        id: "gm-light",
        name: "Gruvbox Material Light",
        sha256: "eba8a362ed0cfcf78960e0f7439560a09d0bb74fb5de650c3db6fb355c4fa62c",
        size_bytes: 378868,
    },
];

/// The background choices, in the order Settings shows them.
pub const BACKGROUNDS: &[(&str, &str)] = &[
    ("art", "Art"),
    ("live", "Animated"),
    ("transparent", "Glass"),
    ("solid", "Solid"),
];

/// The settings a background sets. A theme's own changes never name them, so switching the
/// background after a theme leaves its colours alone.
pub const BACKGROUND_KEYS: &[&str] = &[
    "windowArt",
    "windowArtLight",
    "windowArtDim",
    "windowArtBlur",
    "windowGlass",
    "windowGlassSource",
    "windowGlassLiveStyleDark",
    "windowGlassLiveStyleLight",
    "windowGlassLiveSpeed",
    "windowGlassLiveBrightness",
    "windowGlassSidebarOpacityDark",
    "windowGlassWorkAreaTintDark",
    "windowGlassSidebarOpacityLight",
    "windowGlassWorkAreaTintLight",
];

/// The tints the artwork shows through under Art, sidebar and work area (the art themes' own, 86
/// and 76 in dark mode; light artwork is pale, so light mode covers less of it), and the dimming
/// it starts with when the theme names none.
const ART_TINTS_DARK: [(&str, &str); 2] = [
    ("windowGlassSidebarOpacityDark", "86"),
    ("windowGlassWorkAreaTintDark", "76"),
];
const ART_TINTS_LIGHT: [(&str, &str); 2] = [
    ("windowGlassSidebarOpacityLight", "80"),
    ("windowGlassWorkAreaTintLight", "68"),
];
const ART_DIM_DARK: &str = "45";
const ART_DIM_LIGHT: &str = "12";

/// The glass tints keys of one appearance.
const TINT_KEYS_DARK: [&str; 2] = [
    "windowGlassSidebarOpacityDark",
    "windowGlassWorkAreaTintDark",
];
const TINT_KEYS_LIGHT: [&str; 2] = [
    "windowGlassSidebarOpacityLight",
    "windowGlassWorkAreaTintLight",
];

/// Colourfulness Full, as a preset writes it (`colourfulness_patch` at its last position in the
/// desktop's settings_modal/tabs/theme/colours.rs): contrast -24 for both areas, the custom dark
/// colour's depth 96 - 12 held to its 85 minimum, the light one's lightness 96 - 12.
const FULL_COLOURFULNESS: [(&str, &str); 4] = [
    ("themeSidebarContrast", "-24"),
    ("themeWorkAreaContrast", "-24"),
    ("customSidebarTitlebarBackgroundDarknessPercent", "85"),
    ("customSidebarTitlebarLightBackgroundLightnessPercent", "84"),
];

fn artwork(id: &str) -> Option<&'static str> {
    let id = if id == DEFAULT_LOOK_ID {
        crate::default_value("windowArt")
            .and_then(J::as_str)
            .filter(|art| *art != "none")
            .unwrap_or("inkstone")
    } else {
        id
    };
    artwork_by_id(id).map(|artwork| artwork.id)
}

fn number_text(number: f64) -> String {
    Json::Num(number).to_pretty_string()
}

/// A setting's catalog default for `platform`, written the way `ghostex theme set` takes it.
fn default_text(platform: Platform, key: &str) -> Option<String> {
    if let Some((appearance, role)) = key.split_once('.') {
        let roles = crate::default_value(crate::theme_roles::THEME_ROLES_SETTING)?;
        return Some(
            roles
                .get(appearance)
                .and_then(|roles| roles.get(role))
                .and_then(J::as_str)
                .unwrap_or("default")
                .to_string(),
        );
    }
    Some(
        match crate::availability::default_value_on(platform, key)? {
            J::Str(text) => text.to_string(),
            J::Bool(flag) => flag.to_string(),
            J::Num(number) => number_text(*number),
            _ => return None,
        },
    )
}

/// Ordered `key -> value` changes.
#[derive(Default, Clone)]
struct Changes(Vec<(String, String)>);

impl Changes {
    fn set(&mut self, key: &str, value: &str) {
        match self.0.iter_mut().find(|(name, _)| name == key) {
            Some(entry) => entry.1 = value.to_string(),
            None => self.0.push((key.to_string(), value.to_string())),
        }
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    /// Every theme colour of `appearance` that is not set yet takes the clean fill
    /// (`CLEAN_THEME_FILLS`), or goes back to the theme's own where there is none, so a colour an
    /// earlier theme set never lingers.
    ///
    /// CDXC:Theming 2026-11-25 DECISION:
    /// User: "fix the color scheme for all the themes and make them look good ... clean and minimal", choosing "Clean pass, keep identity": every theme (Default, the art themes, the agent-made looks and the presets) takes the clean Default's borders, dividers, card, control, menu and chat box fills, hover and selected row for the colours it does not set itself, and keeps its own window colour, text, grey text, accent, links, status colours and terminal palette. Before, a colour a preset or look left unset was taken off (`default`), which fell back to the older built-in fills.
    /// CDXC:Theming 2026-11-25 WHY: the clean fills were the catalog's default `themeRoles` in PR 757, but the user kept their own default look (Polar Night's colours in `themeRoles`) when it was merged, so the fills have their own list here instead of following the default.
    fn fill_roles(&mut self, appearance: &str) {
        let clean = CLEAN_THEME_FILLS
            .iter()
            .find(|(mode, _)| *mode == appearance)
            .map(|(_, fills)| *fills);
        for role in THEME_ROLES {
            let key = format!("{appearance}.{}", role.id);
            if self.get(&key).is_none() {
                let value = clean
                    .and_then(|fills| fills.iter().find(|(id, _)| *id == role.id))
                    .map(|(_, value)| *value)
                    .unwrap_or("default");
                self.set(&key, value);
            }
        }
    }

    fn json(&self) -> Json {
        Json::obj(
            self.0
                .iter()
                .map(|(key, value)| (key.clone(), Json::str(value.clone()))),
        )
    }
}

/// One theme as the list carries it.
struct Theme {
    id: String,
    name: String,
    label: Option<String>,
    kind: &'static str,
    family: Option<String>,
    dark: bool,
    partner: Option<String>,
    prompt: Option<String>,
    changes: Changes,
    partner_changes: Option<Changes>,
    /// The Live animation: style, speed and brightness.
    live: Option<(String, String, String)>,
    /// The art dimming and blur it shows its artwork with.
    art_look: Option<(String, String)>,
    /// Its glass tints (sidebar, work area) for Live and Transparent; the shipped ones when it
    /// names none.
    tints: [String; 2],
    default_background: Option<&'static str>,
}

impl Theme {
    fn art_key(dark: bool) -> &'static str {
        if dark {
            "windowArt"
        } else {
            "windowArtLight"
        }
    }

    fn backgrounds(&self) -> Vec<&'static str> {
        BACKGROUNDS
            .iter()
            .map(|(id, _)| *id)
            .filter(|id| match *id {
                "art" => artwork(&self.id).is_some(),
                "live" => self.live.is_some(),
                _ => true,
            })
            .collect()
    }

    /// What `background` changes for this theme's own appearance.
    fn background_changes(&self, background: &str) -> Changes {
        let mut changes = Changes::default();
        let art_key = Self::art_key(self.dark);
        match background {
            "art" => {
                if let Some(artwork) = artwork(&self.id) {
                    changes.set(art_key, artwork);
                }
                if self.kind == "default" {
                    if let Some(light_art) = crate::default_value("windowArtLight")
                        .and_then(J::as_str)
                        .filter(|art| !art.is_empty())
                    {
                        changes.set("windowArtLight", light_art);
                    }
                }
                let (dim, blur) = self.art_look.clone().unwrap_or_else(|| {
                    let dim = if self.dark {
                        ART_DIM_DARK
                    } else {
                        ART_DIM_LIGHT
                    };
                    (dim.to_string(), "0".to_string())
                });
                changes.set("windowArtDim", &dim);
                changes.set("windowArtBlur", &blur);
                if self.kind == "default" {
                    self.set_tints(&mut changes);
                } else {
                    let tints = if self.dark {
                        ART_TINTS_DARK
                    } else {
                        ART_TINTS_LIGHT
                    };
                    for (key, value) in tints {
                        changes.set(key, value);
                    }
                }
            }
            "live" => {
                self.set_tints(&mut changes);
                changes.set(art_key, "none");
                changes.set("windowGlass", "frosted");
                changes.set("windowGlassSource", "live");
                if let Some((style, speed, brightness)) = &self.live {
                    changes.set("windowGlassLiveStyleDark", style);
                    changes.set("windowGlassLiveSpeed", speed);
                    changes.set("windowGlassLiveBrightness", brightness);
                }
            }
            "transparent" => {
                self.set_tints(&mut changes);
                changes.set(art_key, "none");
                changes.set("windowGlass", "frosted");
                changes.set("windowGlassSource", "desktopAndWindows");
            }
            _ => {
                changes.set(art_key, "none");
                changes.set("windowGlass", "opaque");
            }
        }
        changes
    }

    fn set_tints(&self, changes: &mut Changes) {
        let keys = if self.dark {
            TINT_KEYS_DARK
        } else {
            TINT_KEYS_LIGHT
        };
        for (key, value) in keys.iter().zip(&self.tints) {
            changes.set(key, value);
        }
    }

    /// What `background` changes for the partner's appearance, when the partner comes along.
    fn partner_background_changes(&self, background: &str) -> Option<Changes> {
        let partner = self.partner.as_deref()?;
        let mut changes = Changes::default();
        let key = Self::art_key(!self.dark);
        match (background, artwork(partner)) {
            ("art", Some(artwork)) => changes.set(key, artwork),
            ("art", None) => return None,
            _ => changes.set(key, "none"),
        }
        Some(changes)
    }

    fn json(&self) -> Json {
        let mut fields: Vec<(String, Json)> = vec![
            ("id".into(), Json::str(&self.id)),
            ("name".into(), Json::str(&self.name)),
            ("kind".into(), Json::str(self.kind)),
            (
                "mode".into(),
                Json::str(if self.dark { "dark" } else { "light" }),
            ),
        ];
        if let Some(label) = &self.label {
            fields.push(("label".into(), Json::str(label)));
        }
        if let Some(family) = &self.family {
            fields.push(("family".into(), Json::str(family)));
        }
        if let Some(partner) = &self.partner {
            fields.push(("partner".into(), Json::str(partner)));
        }
        if self.kind == "art" {
            fields.push(("art".into(), Json::Bool(true)));
        }
        if let Some(prompt) = &self.prompt {
            fields.push(("prompt".into(), Json::str(prompt)));
        }
        fields.push(("changes".into(), self.changes.json()));
        if let Some(partner) = &self.partner_changes {
            fields.push(("partnerChanges".into(), partner.json()));
        }
        let backgrounds = self.backgrounds();
        fields.push((
            "backgrounds".into(),
            Json::Arr(backgrounds.iter().map(|id| Json::str(*id)).collect()),
        ));
        fields.push((
            "defaultBackground".into(),
            self.default_background
                .filter(|background| backgrounds.contains(background))
                .map(Json::str)
                .unwrap_or(Json::Null),
        ));
        fields.push((
            "backgroundChanges".into(),
            Json::obj(
                backgrounds
                    .iter()
                    .map(|id| (*id, self.background_changes(id).json())),
            ),
        ));
        let partner: Vec<(&str, Json)> = backgrounds
            .iter()
            .filter_map(|id| {
                self.partner_background_changes(id)
                    .map(|changes| (*id, changes.json()))
            })
            .collect();
        if !partner.is_empty() {
            fields.push(("partnerBackgroundChanges".into(), Json::obj(partner)));
        }
        Json::Obj(fields)
    }
}

fn look_text(look: &J, key: &str) -> Option<String> {
    look.get(key).and_then(J::as_str).map(str::to_string)
}

/// An agent-made look or art theme from `THEME_GALLERY`, its background settings set apart.
fn gallery_theme(look: &J) -> Theme {
    let mut changes = Changes::default();
    let mut all = Changes::default();
    for (key, value) in look.get("changes").map(J::entries).unwrap_or_default() {
        let value = value.as_str().unwrap_or_default();
        all.set(key, value);
        if !BACKGROUND_KEYS.contains(key) {
            changes.set(key, value);
        }
    }
    changes.fill_roles("dark");
    let id = look_text(look, "id").unwrap_or_default();
    let has_artwork = artwork(&id).is_some();
    let art = look.get("art").and_then(|art| match art {
        J::Bool(flag) => Some(*flag),
        _ => None,
    }) == Some(true);
    let live = all.get("windowGlassLiveStyleDark").map(|style| {
        (
            style.to_string(),
            all.get("windowGlassLiveSpeed").unwrap_or("1").to_string(),
            all.get("windowGlassLiveBrightness")
                .unwrap_or("80")
                .to_string(),
        )
    });
    Theme {
        id,
        name: look_text(look, "name").unwrap_or_default(),
        label: look_text(look, "label"),
        kind: if art { "art" } else { "look" },
        family: None,
        dark: true,
        partner: None,
        prompt: look_text(look, "prompt"),
        art_look: has_artwork.then(|| {
            (
                all.get("windowArtDim").unwrap_or(ART_DIM_DARK).to_string(),
                all.get("windowArtBlur").unwrap_or("0").to_string(),
            )
        }),
        tints: TINT_KEYS_DARK.map(|key| {
            all.get(key)
                .map(str::to_string)
                .unwrap_or_else(|| default_text(Platform::current(), key).unwrap_or_default())
        }),
        live,
        changes,
        partner_changes: None,
        default_background: Some(if has_artwork { "art" } else { "live" }),
    }
}

/// Default: every key a look sets back to `platform`'s shipped value, its Live animation the
/// shipped one, and its background the shipped one (Live, or Solid where transparency is off out
/// of the box).
fn default_theme(platform: Platform) -> Theme {
    let mut keys: Vec<&'static str> = Vec::new();
    for look in THEME_GALLERY.as_array() {
        for (key, _) in look.get("changes").map(J::entries).unwrap_or_default() {
            if !keys.contains(key) && !BACKGROUND_KEYS.contains(key) {
                keys.push(key);
            }
        }
    }
    let mut changes = Changes::default();
    for key in keys {
        if let Some(value) = default_text(platform, key) {
            changes.set(key, &value);
        }
    }
    changes.fill_roles("dark");
    let shipped = |key: &str| default_text(platform, key).unwrap_or_default();
    let solid = shipped("windowGlass") == "opaque";
    let has_default_art = artwork_by_id(&shipped("windowArt")).is_some();
    Theme {
        id: DEFAULT_LOOK_ID.into(),
        name: DEFAULT_LOOK_NAME.into(),
        label: None,
        kind: "default",
        family: None,
        dark: true,
        partner: None,
        prompt: Some(
            "Put Ghostex's own look back: its default colours, artwork, glass and Animated background.".into(),
        ),
        live: Some((
            shipped("windowGlassLiveStyleDark"),
            shipped("windowGlassLiveSpeed"),
            shipped("windowGlassLiveBrightness"),
        )),
        art_look: has_default_art.then(|| (shipped("windowArtDim"), shipped("windowArtBlur"))),
        tints: TINT_KEYS_DARK.map(|key| shipped(key)),
        changes,
        partner_changes: None,
        default_background: Some(if has_default_art {
            "art"
        } else if solid {
            "solid"
        } else {
            "live"
        }),
    }
}

/// The keys one appearance of a preset writes: its colour, tint, terminal palette and theme
/// colours (the published palette's, `PRESET_ROLES`, every other colour back to the theme's own).
fn preset_mode_changes(preset: &J, changes: &mut Changes) {
    let text = |key: &str| preset.get(key).and_then(J::as_str).unwrap_or_default();
    let dark = text("mode") == "dark";
    let (appearance, colour, tint, palette) = if dark {
        (
            "dark",
            "darkThemePreset",
            "customSidebarTitlebarBackgroundTintColor",
            "terminalGhosttyTheme",
        )
    } else {
        (
            "light",
            "lightThemePreset",
            "customSidebarTitlebarLightBackgroundTintColor",
            "terminalGhosttyLightTheme",
        )
    };
    changes.set(colour, "custom");
    changes.set(tint, text("tintColor"));
    changes.set(palette, text("terminalPalette"));
    if let Some((_, roles)) = PRESET_ROLES.iter().find(|(id, _)| *id == text("id")) {
        for (role, value) in *roles {
            changes.set(&format!("{appearance}.{role}"), value);
            if *role == "accent" {
                changes.set(&format!("{appearance}.focusRing"), value);
            }
        }
    }
    changes.fill_roles(appearance);
}

/// A named preset: its appearance, its own colours and Colourfulness Full, and its partner's
/// colours apart, for the caller to add when the partner follows (CDXC:Theming 2026-11-01 in the
/// desktop's settings_modal/tabs/theme/presets.rs).
fn preset_theme(preset: &J) -> Theme {
    let text = |key: &str| look_text(preset, key);
    let dark = text("mode").as_deref() == Some("dark");
    let mut changes = Changes::default();
    changes.set("sidebarTheme", if dark { "dark-2" } else { "plain-light" });
    preset_mode_changes(preset, &mut changes);
    for (key, value) in FULL_COLOURFULNESS {
        changes.set(key, value);
    }
    changes.set("terminalBackgroundMode", "theme");
    let partner = text("partner");
    let partner_changes = partner.as_deref().and_then(|partner| {
        let other = THEME_PRESETS
            .as_array()
            .iter()
            .find(|other| other.get("id").and_then(J::as_str) == Some(partner))?;
        let mut changes = Changes::default();
        preset_mode_changes(other, &mut changes);
        Some(changes)
    });
    Theme {
        id: text("id").unwrap_or_default(),
        name: text("name").unwrap_or_default(),
        label: text("label"),
        kind: "preset",
        family: text("family"),
        dark,
        partner,
        prompt: None,
        live: None,
        art_look: None,
        tints: (if dark {
            TINT_KEYS_DARK
        } else {
            TINT_KEYS_LIGHT
        })
        .map(|key| default_text(Platform::current(), key).unwrap_or_default()),
        changes,
        partner_changes,
        default_background: None,
    }
}

/// Every theme in the order Settings shows them: Default, the art themes, the agent-made looks,
/// then the named presets.
pub fn theme_gallery(platform: Platform) -> Json {
    let mut themes = vec![default_theme(platform)];
    themes.extend(THEME_GALLERY.as_array().iter().map(gallery_theme));
    themes.extend(THEME_PRESETS.as_array().iter().map(preset_theme));
    Json::Arr(themes.iter().map(Theme::json).collect())
}
