//! Settings > Experience: Chill, Regular or Advanced for the whole app, then one row per area (the
//! chat box, the chat, the sessions list). Each row is selected while its settings match a preset
//! and says Custom when they match none; picking one writes that preset's settings
//! (`ghostex_settings_catalog::experience`, where the user's decisions are).
use super::super::super::catalog::SettingOption;
use super::super::super::fields::{RowSpec, custom_note, segmented_field, settings_section};
use super::super::super::page::PageBlock;
use super::super::super::store::SettingsStore;
use super::{GeneralCx, GeneralTab};
use ghostex_settings_catalog::experience::{self, Area, Preset};
use gpui::{AnyElement, Context, Window};
use serde_json::Value;

fn options() -> Vec<SettingOption> {
    Preset::ALL
        .into_iter()
        .map(|preset| SettingOption {
            label: preset.label().to_string(),
            value: preset.id().to_string(),
        })
        .collect()
}

/// Writes every setting of `writes` in one save, as the Sidebar's Preset row does.
fn apply(page: &mut GeneralTab, writes: Vec<(&'static str, Value)>, cx: &mut Context<GeneralTab>) {
    let store = page.store.clone();
    store.update(cx, |store: &mut SettingsStore, cx| {
        let mut settings = store.settings().clone();
        for (key, value) in writes {
            settings.insert(key.to_string(), value);
        }
        store.apply_settings(settings, "settings:bulk", cx);
    });
}

fn area_row(
    g: &GeneralCx,
    area: &'static Area,
    id: &'static str,
    value: &dyn Fn(&str) -> Value,
    cx: &mut Context<GeneralTab>,
) -> AnyElement {
    let shown = experience::shown(area, value).map(Preset::id);
    let spec = RowSpec::new(area.label).description(area.description);
    segmented_field(
        &g.p,
        id,
        spec,
        None,
        &options(),
        shown,
        shown.is_none().then(|| custom_note(&g.p)),
        move |page: &mut GeneralTab, next, _window, cx| {
            if let Some(preset) = Preset::from_id(&next) {
                apply(page, experience::writes(area, preset), cx);
            }
        },
        cx,
    )
}

pub(super) fn sections(
    _page: &mut GeneralTab,
    g: &GeneralCx,
    _window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Vec<PageBlock> {
    if !g.search.subsection_visible("experience", g.power_visible) {
        return Vec::new();
    }
    let values = g.values.clone();
    let value = move |key: &str| values.value(key);
    let s = "experience";
    let mut blocks = Vec::new();
    // CDXC:Experience 2026-11-15 DECISION (Q3284, in packages/settings-catalog/src/experience.rs):
    // the whole app's choice selects a preset only while every area matches it, and says Custom
    // beside the three otherwise.
    if g.visible(s, "experiencePreset") {
        let overall = experience::overall(&value).map(Preset::id);
        let spec = RowSpec::new("Experience").description(
            "How many buttons and details Ghostex shows. Chill: a simple chat box and a calm chat. Regular: pick the model, the mode and the effort as you chat. Advanced: for people who use terminals more. Every switch stays in Settings.",
        );
        let row = segmented_field(
            &g.p,
            "experiencePreset",
            spec,
            None,
            &options(),
            overall,
            overall.is_none().then(|| custom_note(&g.p)),
            |page: &mut GeneralTab, next, _window, cx| {
                if let Some(preset) = Preset::from_id(&next) {
                    apply(page, experience::writes_all(preset), cx);
                }
            },
            cx,
        );
        blocks.extend(
            settings_section(
                &g.p,
                g.section_title(s, "Experience"),
                None,
                None,
                vec![row],
            )
            .map(|section| PageBlock::section(s, section)),
        );
    }
    let rows: Vec<AnyElement> = [
        ("chatBox", "experienceChatBox"),
        ("chat", "experienceChat"),
        ("sessionsList", "experienceSessionsList"),
    ]
    .into_iter()
    .filter(|(_, key)| g.visible(s, key))
    .filter_map(|(area, key)| experience::area(area).map(|area| (area, key)))
    .map(|(area, key)| area_row(g, area, key, &value, cx))
    .collect();
    blocks.extend(
        settings_section(
            &g.p,
            "Areas",
            Some(
                "Each area follows Experience unless you pick one for it here. Change one of its settings by hand and it says Custom."
                    .into(),
            ),
            None,
            rows,
        )
        .map(|section| PageBlock::section("experienceAreas", section)),
    );
    blocks
}
