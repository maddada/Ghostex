use crate::json::J;

/// The theme presets Settings > Interface offers above the colours: each sets the existing colour,
/// Colourfulness, terminal palette and terminal background settings so the app looks like that
/// theme. A preset is a one-time write of those settings, not a saved choice: nothing records
/// which preset was picked, and every value stays editable afterwards.
///
/// Per preset: `id`; `name` (the theme's own name); `family` and `label` (how Settings groups and
/// labels the card); `mode` (`dark` or `light`); `partner` (the same family's preset for the other
/// mode, set along when that mode still follows, or null); `background` (the theme's own
/// background, which the work area, chat and a theme-coloured terminal show at Full);
/// `tintColor` (the colour Colourfulness Full paints the sidebar with: `background` with the work
/// area's fixed step toward white taken out, 1% in dark mode and 25% in light mode);
/// `terminalPalette` (the bundled palette in `.dependencies/theme-presets/palettes/`); and
/// `swatch` (ANSI colours 1 to 4 of that palette, for the card).
///
/// CDXC:Theming 2026-11-01 WHY:
/// Solarized Dark's background `#002b36` has red 0, which no dark colour reaches after the 1%
/// step toward white (the least is 3), so its work area is `#032b36`; every other preset's work
/// area is its theme background exactly. The terminal palette keeps `#002b36`.
pub const THEME_PRESETS: J = J::Arr(&[
    J::Obj(&[
        ("id", J::Str("mocha")),
        ("name", J::Str("Catppuccin Mocha")),
        ("family", J::Str("Catppuccin")),
        ("label", J::Str("Mocha")),
        ("mode", J::Str("dark")),
        ("partner", J::Str("latte")),
        ("background", J::Str("#1e1e2e")),
        ("tintColor", J::Str("#1c1c2c")),
        ("terminalPalette", J::Str("Ghostex Catppuccin Mocha")),
        (
            "swatch",
            J::Arr(&[
                J::Str("#f38ba8"),
                J::Str("#a6e3a1"),
                J::Str("#f9e2af"),
                J::Str("#89b4fa"),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("macchiato")),
        ("name", J::Str("Catppuccin Macchiato")),
        ("family", J::Str("Catppuccin")),
        ("label", J::Str("Macchiato")),
        ("mode", J::Str("dark")),
        ("partner", J::Str("latte")),
        ("background", J::Str("#24273a")),
        ("tintColor", J::Str("#222538")),
        ("terminalPalette", J::Str("Ghostex Catppuccin Macchiato")),
        (
            "swatch",
            J::Arr(&[
                J::Str("#ed8796"),
                J::Str("#a6da95"),
                J::Str("#eed49f"),
                J::Str("#8aadf4"),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("frappe")),
        ("name", J::Str("Catppuccin Frappé")),
        ("family", J::Str("Catppuccin")),
        ("label", J::Str("Frappé")),
        ("mode", J::Str("dark")),
        ("partner", J::Str("latte")),
        ("background", J::Str("#303446")),
        ("tintColor", J::Str("#2e3244")),
        ("terminalPalette", J::Str("Ghostex Catppuccin Frappe")),
        (
            "swatch",
            J::Arr(&[
                J::Str("#e78284"),
                J::Str("#a6d189"),
                J::Str("#e5c890"),
                J::Str("#8caaee"),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("latte")),
        ("name", J::Str("Catppuccin Latte")),
        ("family", J::Str("Catppuccin")),
        ("label", J::Str("Latte")),
        ("mode", J::Str("light")),
        ("partner", J::Str("mocha")),
        ("background", J::Str("#eff1f5")),
        ("tintColor", J::Str("#eaecf2")),
        ("terminalPalette", J::Str("Ghostex Catppuccin Latte")),
        (
            "swatch",
            J::Arr(&[
                J::Str("#d20f39"),
                J::Str("#40a02b"),
                J::Str("#df8e1d"),
                J::Str("#1e66f5"),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("dracula")),
        ("name", J::Str("Dracula")),
        ("family", J::Str("Dracula")),
        ("label", J::Str("Dracula")),
        ("mode", J::Str("dark")),
        ("partner", J::Str("alucard")),
        ("background", J::Str("#282a36")),
        ("tintColor", J::Str("#262834")),
        ("terminalPalette", J::Str("Ghostex Dracula")),
        (
            "swatch",
            J::Arr(&[
                J::Str("#ff5555"),
                J::Str("#50fa7b"),
                J::Str("#f1fa8c"),
                J::Str("#bd93f9"),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("alucard")),
        ("name", J::Str("Alucard")),
        ("family", J::Str("Dracula")),
        ("label", J::Str("Alucard")),
        ("mode", J::Str("light")),
        ("partner", J::Str("dracula")),
        ("background", J::Str("#fffbeb")),
        ("tintColor", J::Str("#fffae4")),
        ("terminalPalette", J::Str("Ghostex Alucard")),
        (
            "swatch",
            J::Arr(&[
                J::Str("#cb3a2a"),
                J::Str("#14710a"),
                J::Str("#846e15"),
                J::Str("#644ac9"),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("sol-dark")),
        ("name", J::Str("Solarized Dark")),
        ("family", J::Str("Solarized")),
        ("label", J::Str("Solarized Dark")),
        ("mode", J::Str("dark")),
        ("partner", J::Str("sol-light")),
        ("background", J::Str("#002b36")),
        ("tintColor", J::Str("#002934")),
        ("terminalPalette", J::Str("Ghostex Solarized Dark")),
        (
            "swatch",
            J::Arr(&[
                J::Str("#dc322f"),
                J::Str("#859900"),
                J::Str("#b58900"),
                J::Str("#268bd2"),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("sol-light")),
        ("name", J::Str("Solarized Light")),
        ("family", J::Str("Solarized")),
        ("label", J::Str("Solarized Light")),
        ("mode", J::Str("light")),
        ("partner", J::Str("sol-dark")),
        ("background", J::Str("#fdf6e3")),
        ("tintColor", J::Str("#fcf3da")),
        ("terminalPalette", J::Str("Ghostex Solarized Light")),
        (
            "swatch",
            J::Arr(&[
                J::Str("#dc322f"),
                J::Str("#859900"),
                J::Str("#b58900"),
                J::Str("#268bd2"),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("tn-night")),
        ("name", J::Str("Tokyo Night Night")),
        ("family", J::Str("Tokyo Night")),
        ("label", J::Str("Night")),
        ("mode", J::Str("dark")),
        ("partner", J::Str("tn-day")),
        ("background", J::Str("#1a1b26")),
        ("tintColor", J::Str("#181924")),
        ("terminalPalette", J::Str("Ghostex Tokyo Night Night")),
        (
            "swatch",
            J::Arr(&[
                J::Str("#f7768e"),
                J::Str("#9ece6a"),
                J::Str("#e0af68"),
                J::Str("#7aa2f7"),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("tn-storm")),
        ("name", J::Str("Tokyo Night Storm")),
        ("family", J::Str("Tokyo Night")),
        ("label", J::Str("Storm")),
        ("mode", J::Str("dark")),
        ("partner", J::Str("tn-day")),
        ("background", J::Str("#24283b")),
        ("tintColor", J::Str("#222639")),
        ("terminalPalette", J::Str("Ghostex Tokyo Night Storm")),
        (
            "swatch",
            J::Arr(&[
                J::Str("#f7768e"),
                J::Str("#9ece6a"),
                J::Str("#e0af68"),
                J::Str("#7aa2f7"),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("tn-moon")),
        ("name", J::Str("Tokyo Night Moon")),
        ("family", J::Str("Tokyo Night")),
        ("label", J::Str("Moon")),
        ("mode", J::Str("dark")),
        ("partner", J::Str("tn-day")),
        ("background", J::Str("#222436")),
        ("tintColor", J::Str("#202234")),
        ("terminalPalette", J::Str("Ghostex Tokyo Night Moon")),
        (
            "swatch",
            J::Arr(&[
                J::Str("#ff757f"),
                J::Str("#c3e88d"),
                J::Str("#ffc777"),
                J::Str("#82aaff"),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("tn-day")),
        ("name", J::Str("Tokyo Night Day")),
        ("family", J::Str("Tokyo Night")),
        ("label", J::Str("Day")),
        ("mode", J::Str("light")),
        ("partner", J::Str("tn-night")),
        ("background", J::Str("#e1e2e7")),
        ("tintColor", J::Str("#d7d8df")),
        ("terminalPalette", J::Str("Ghostex Tokyo Night Day")),
        (
            "swatch",
            J::Arr(&[
                J::Str("#f52a65"),
                J::Str("#587539"),
                J::Str("#8c6c3e"),
                J::Str("#2e7de9"),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("nord")),
        ("name", J::Str("Nord")),
        ("family", J::Str("Nord")),
        ("label", J::Str("Nord")),
        ("mode", J::Str("dark")),
        ("partner", J::Null),
        ("background", J::Str("#2e3440")),
        ("tintColor", J::Str("#2c323e")),
        ("terminalPalette", J::Str("Ghostex Nord")),
        (
            "swatch",
            J::Arr(&[
                J::Str("#bf616a"),
                J::Str("#a3be8c"),
                J::Str("#ebcb8b"),
                J::Str("#81a1c1"),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("kg-wave")),
        ("name", J::Str("Kanagawa Wave")),
        ("family", J::Str("Kanagawa")),
        ("label", J::Str("Wave")),
        ("mode", J::Str("dark")),
        ("partner", J::Str("kg-lotus")),
        ("background", J::Str("#1f1f28")),
        ("tintColor", J::Str("#1d1d26")),
        ("terminalPalette", J::Str("Ghostex Kanagawa Wave")),
        (
            "swatch",
            J::Arr(&[
                J::Str("#c34043"),
                J::Str("#76946a"),
                J::Str("#c0a36e"),
                J::Str("#7e9cd8"),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("kg-dragon")),
        ("name", J::Str("Kanagawa Dragon")),
        ("family", J::Str("Kanagawa")),
        ("label", J::Str("Dragon")),
        ("mode", J::Str("dark")),
        ("partner", J::Str("kg-lotus")),
        ("background", J::Str("#181616")),
        ("tintColor", J::Str("#161414")),
        ("terminalPalette", J::Str("Ghostex Kanagawa Dragon")),
        (
            "swatch",
            J::Arr(&[
                J::Str("#c4746e"),
                J::Str("#8a9a7b"),
                J::Str("#c4b28a"),
                J::Str("#8ba4b0"),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("kg-lotus")),
        ("name", J::Str("Kanagawa Lotus")),
        ("family", J::Str("Kanagawa")),
        ("label", J::Str("Lotus")),
        ("mode", J::Str("light")),
        ("partner", J::Str("kg-wave")),
        ("background", J::Str("#f2ecbc")),
        ("tintColor", J::Str("#eee6a6")),
        ("terminalPalette", J::Str("Ghostex Kanagawa Lotus")),
        (
            "swatch",
            J::Arr(&[
                J::Str("#c84053"),
                J::Str("#6f894e"),
                J::Str("#77713f"),
                J::Str("#4d699b"),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("ef-dark")),
        ("name", J::Str("Everforest Dark")),
        ("family", J::Str("Everforest")),
        ("label", J::Str("Everforest Dark")),
        ("mode", J::Str("dark")),
        ("partner", J::Str("ef-light")),
        ("background", J::Str("#2d353b")),
        ("tintColor", J::Str("#2b3339")),
        ("terminalPalette", J::Str("Ghostex Everforest Dark")),
        (
            "swatch",
            J::Arr(&[
                J::Str("#e67e80"),
                J::Str("#a7c080"),
                J::Str("#dbbc7f"),
                J::Str("#7fbbb3"),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("ef-light")),
        ("name", J::Str("Everforest Light")),
        ("family", J::Str("Everforest")),
        ("label", J::Str("Everforest Light")),
        ("mode", J::Str("light")),
        ("partner", J::Str("ef-dark")),
        ("background", J::Str("#fdf6e3")),
        ("tintColor", J::Str("#fcf3da")),
        ("terminalPalette", J::Str("Ghostex Everforest Light")),
        (
            "swatch",
            J::Arr(&[
                J::Str("#f85552"),
                J::Str("#8da101"),
                J::Str("#dfa000"),
                J::Str("#3a94c5"),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("gm-dark")),
        ("name", J::Str("Gruvbox Material Dark")),
        ("family", J::Str("Gruvbox Material")),
        ("label", J::Str("Gruvbox Material Dark")),
        ("mode", J::Str("dark")),
        ("partner", J::Str("gm-light")),
        ("background", J::Str("#282828")),
        ("tintColor", J::Str("#262626")),
        ("terminalPalette", J::Str("Ghostex Gruvbox Material Dark")),
        (
            "swatch",
            J::Arr(&[
                J::Str("#ea6962"),
                J::Str("#a9b665"),
                J::Str("#d8a657"),
                J::Str("#7daea3"),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("gm-light")),
        ("name", J::Str("Gruvbox Material Light")),
        ("family", J::Str("Gruvbox Material")),
        ("label", J::Str("Gruvbox Material Light")),
        ("mode", J::Str("light")),
        ("partner", J::Str("gm-dark")),
        ("background", J::Str("#fbf1c7")),
        ("tintColor", J::Str("#faecb4")),
        ("terminalPalette", J::Str("Ghostex Gruvbox Material Light")),
        (
            "swatch",
            J::Arr(&[
                J::Str("#c14a4a"),
                J::Str("#6c782e"),
                J::Str("#b47109"),
                J::Str("#45707a"),
            ]),
        ),
    ]),
]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dark_chrome::{
        relative_luminance, FULL_COLOUR_MAX_LUMINANCE, LIGHT_FULL_COLOUR_MIN_LUMINANCE,
    };
    use crate::data::GHOSTTY_THEME_SETTING_OPTIONS;

    fn text(preset: &J, key: &str) -> &'static str {
        preset
            .get(key)
            .and_then(J::as_str)
            .unwrap_or_else(|| panic!("{key} missing"))
    }

    fn rgb(hex: &str) -> [f64; 3] {
        assert!(hex.len() == 7 && hex.starts_with('#'), "{hex}");
        [1, 3, 5].map(|start| f64::from(u8::from_str_radix(&hex[start..start + 2], 16).unwrap()))
    }

    #[test]
    fn twenty_presets_with_unique_ids_and_matching_partners() {
        let presets = THEME_PRESETS.as_array();
        assert_eq!(presets.len(), 20);
        let mut ids: Vec<&str> = presets.iter().map(|preset| text(preset, "id")).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 20);
        let dark = presets
            .iter()
            .filter(|preset| text(preset, "mode") == "dark")
            .count();
        assert_eq!((dark, presets.len() - dark), (13, 7));
        for preset in presets {
            let id = text(preset, "id");
            assert!(matches!(text(preset, "mode"), "dark" | "light"), "{id}");
            match preset.get("partner") {
                Some(J::Str(partner)) => {
                    let other = presets
                        .iter()
                        .find(|other| text(other, "id") == *partner)
                        .unwrap_or_else(|| panic!("{id}: partner {partner} missing"));
                    assert_ne!(text(other, "mode"), text(preset, "mode"), "{id}");
                    assert_eq!(text(other, "family"), text(preset, "family"), "{id}");
                }
                Some(J::Null) => assert_eq!(id, "nord"),
                other => panic!("{id}: partner {other:?}"),
            }
            assert_eq!(
                preset.get("swatch").map(J::as_array).map(<[J]>::len),
                Some(4),
                "{id}"
            );
        }
    }

    #[test]
    fn every_palette_is_offered_and_every_tint_paints_unchanged_at_full() {
        for preset in THEME_PRESETS.as_array() {
            let id = text(preset, "id");
            let palette = text(preset, "terminalPalette");
            assert!(
                GHOSTTY_THEME_SETTING_OPTIONS
                    .iter()
                    .any(|option| option.value == palette),
                "{id}: {palette} is not offered"
            );
            let luminance = relative_luminance(rgb(text(preset, "tintColor")));
            if text(preset, "mode") == "dark" {
                assert!(luminance <= FULL_COLOUR_MAX_LUMINANCE, "{id}: {luminance}");
            } else {
                assert!(
                    luminance >= LIGHT_FULL_COLOUR_MIN_LUMINANCE,
                    "{id}: {luminance}"
                );
            }
            rgb(text(preset, "background"));
        }
    }
}
