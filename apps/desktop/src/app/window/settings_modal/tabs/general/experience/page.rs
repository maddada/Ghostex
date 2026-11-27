//! Settings > Experience: a preview of Ghostex on top, the three levels for the whole app under
//! it, then one row per area (the chat box, the chat, the sessions list) that says in plain words
//! what its level shows, with Chill, Regular, Advanced and Custom. Picking any of them, or
//! flipping one of an area's Custom switches, only tries it on the preview; Apply saves what was
//! tried and Cancel puts the saved settings back (`draft.rs`). The presets and the user's earlier
//! decisions are in `ghostex_settings_catalog::experience`.
//!
//! CDXC:Experience 2026-11-21 DECISION:
//! The user, of the first page: "this page needs a rework. it's not clear what each setting changes. also i want to be able to click on each one and change things without applying so i can see stuff. and i want custom for the stuff that this setting customizes to be shown there", then "i want like preview of the app with different parts hiding/showing etc". They picked layout A, Preview first (Q4361): a big preview of the app, the level cards under it, a row per area that says in plain words what it shows with on and off chips and lights its part of the preview while pointed at, and Custom per area opening that area's switches in place. Nothing is saved until Apply. These rows show their words on the page, unlike other Settings rows (CDXC:Settings 2026-09-09 in fields/row.rs), because saying what each level changes is what the page is for. With the pick the user asked that Maximize be called "Maximize chat box" wherever it shows.
use super::super::super::super::super::native_modal_kit::*;
use super::super::super::super::catalog::SettingOption;
use super::super::super::super::fields::{
    ButtonVariant, ROW_GAP, ROW_PADDING_X, RowSpec, custom_note, settings_button, settings_section,
    settings_section_heading, settings_segmented_repress, toggle_field_with,
};
use super::super::super::super::page::PageBlock;
use super::super::super::super::palette::SettingsPalette;
use super::super::super::super::store::{SettingsStore, SettingsValues};
use super::super::{GeneralCx, GeneralTab};
use super::preview::{PreviewState, preview};
use crate::fold_motion::{FoldMotion, fold_duration, render_fold};
use ghostex_settings_catalog::experience::{self, AREAS, Area, Preset};
use ghostex_settings_catalog::power_user_flags;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::Value;
use std::time::Duration;

const SECTION: &str = "experience";
/// How long an area's Custom switches take to open or close.
const CUSTOM_FOLD: Duration = Duration::from_millis(260);

fn saved(page: &GeneralTab, cx: &gpui::App) -> SettingsValues {
    page.store.read(cx).values()
}

/// Tries `writes` on the preview.
fn try_writes(
    page: &mut GeneralTab,
    writes: Vec<(&'static str, Value)>,
    cx: &mut Context<GeneralTab>,
) {
    let values = saved(page, cx);
    let animate = !cx.reduce_motion();
    page.experience.try_writes(&values, writes, animate);
    cx.notify();
}

/// Saves what was tried in one write, as the Sidebar's Preset row does.
fn apply(page: &mut GeneralTab, cx: &mut Context<GeneralTab>) {
    let values = saved(page, cx);
    let writes = page.experience.take_for_apply(&values);
    if writes.is_empty() {
        return;
    }
    let store = page.store.clone();
    store.update(cx, |store: &mut SettingsStore, cx| {
        let mut settings = store.settings().clone();
        for (key, value) in writes {
            settings.insert(key.to_string(), value);
        }
        store.apply_settings(settings, "settings:bulk", cx);
    });
    cx.notify();
}

fn cancel(page: &mut GeneralTab, cx: &mut Context<GeneralTab>) {
    let values = saved(page, cx);
    let animate = !cx.reduce_motion();
    page.experience.cancel(&values, animate);
    cx.notify();
}

fn toggle_custom(page: &mut GeneralTab, area: &'static str, cx: &mut Context<GeneralTab>) {
    let duration = fold_duration(CUSTOM_FOLD.as_millis() as u64, cx.reduce_motion());
    let motion = page
        .experience
        .custom
        .entry(area)
        .or_insert_with(|| FoldMotion::settled(true));
    let open = motion.collapsed();
    motion.fold_to(!open, duration);
    cx.notify();
}

fn level_options() -> Vec<SettingOption> {
    Preset::ALL
        .into_iter()
        .map(|preset| SettingOption {
            label: preset.label().to_string(),
            value: preset.id().to_string(),
        })
        .chain(std::iter::once(SettingOption {
            label: "Custom".to_string(),
            value: "custom".to_string(),
        }))
        .collect()
}

pub(in super::super) fn sections(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Vec<PageBlock> {
    if !g.search.subsection_visible(SECTION, g.power_visible) {
        return Vec::new();
    }
    let saved = g.values.clone();
    if page.experience.animating() || page.experience.custom.values().any(FoldMotion::animating) {
        window.request_animation_frame();
    }
    let mut blocks = Vec::new();
    if g.visible(SECTION, "experiencePreset") {
        blocks.push(PageBlock::section(
            "experiencePreview",
            preview_card(page, &g.p, &saved, cx),
        ));
        blocks.push(PageBlock::section(
            SECTION,
            level_cards(page, &g.p, &saved, cx),
        ));
    }
    let rows: Vec<AnyElement> = [
        ("chatBox", "experienceChatBox"),
        ("chat", "experienceChat"),
        ("sessionsList", "experienceSessionsList"),
    ]
    .into_iter()
    .filter(|(_, key)| g.visible(SECTION, key))
    .filter_map(|(area, _)| experience::area(area))
    .map(|area| area_row(page, &g.p, area, &saved, cx))
    .collect();
    blocks.extend(
        settings_section(
            &g.p,
            "Areas",
            Some(
                "Point at one to see it in the preview. Custom opens the switches it sets.".into(),
            ),
            None,
            rows,
        )
        .map(|section| PageBlock::section("experienceAreas", section)),
    );
    blocks
}

/// The preview, what it is trying, and Cancel and Apply.
fn preview_card(
    page: &GeneralTab,
    p: &SettingsPalette,
    saved: &SettingsValues,
    cx: &mut Context<GeneralTab>,
) -> AnyElement {
    let draft = &page.experience;
    let changed = draft.changed(saved);
    let state = PreviewState {
        shown: draft.parts(saved),
        draft,
        lit: draft.hovered,
    };
    let message: SharedString = if !changed.is_empty() {
        let count = changed.len();
        format!(
            "Trying it out: {count} setting{} would change. Nothing is saved until Apply.",
            if count == 1 { "" } else { "s" }
        )
        .into()
    } else if draft.saved {
        "Saved. This is your Ghostex now.".into()
    } else {
        "This is your Ghostex now. Pick a level or a switch below to try it here first.".into()
    };
    // Picking Advanced for the whole app shows the Power User Flags; leaving it hides them.
    let flags_chip = changed
        .iter()
        .find(|(key, _)| *key == power_user_flags::LEVEL_KEY)
        .and_then(|(_, value)| {
            let on = value.as_str() == Some("advanced");
            let was_on = power_user_flags::level_is_advanced(&|key| saved.value(key));
            (on != was_on).then(|| {
                let sign = if on { "+" } else { "−" };
                chip(
                    p,
                    format!("{sign} {}", power_user_flags::SECTION_TITLE),
                    true,
                )
            })
        });
    let chips = changed.iter().filter_map(|(key, value)| {
        let control = AREAS
            .iter()
            .flat_map(|area| area.controls.iter())
            .find(|control| control.key == *key)?;
        let shown = control.shown(value);
        let on_hover = !shown
            && matches!(
                control.key,
                "hideSessionAgentIconUntilHover" | "hideBrowserFaviconUntilHover"
            );
        let text = format!(
            "{} {}{}",
            if shown { "+" } else { "−" },
            control.label,
            if on_hover { " on hover" } else { "" }
        );
        Some(chip(p, text, true))
    });
    let chips = chips.chain(flags_chip);
    let has_changes = !changed.is_empty();
    v_flex()
        .w_full()
        .p(px(10.0))
        .gap(px(10.0))
        .rounded(px(MODAL_RADIUS_SECTION))
        .border_1()
        .border_color(hsla(p.hairline))
        .bg(hsla(p.raised))
        .child(preview(p, &state))
        .child(
            h_flex()
                .w_full()
                .px(px(4.0))
                .gap(px(10.0))
                .items_center()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .text_color(hsla(if draft.saved && !has_changes {
                            p.foreground
                        } else {
                            p.muted
                        }))
                        .child(message),
                )
                .child(settings_button(
                    p,
                    "experience-cancel",
                    "Cancel",
                    None,
                    ButtonVariant::Default,
                    !has_changes,
                    None,
                    |page: &mut GeneralTab, _window, cx| cancel(page, cx),
                    cx,
                ))
                .child(settings_button(
                    p,
                    "experience-apply",
                    "Apply",
                    None,
                    ButtonVariant::Default,
                    !has_changes,
                    None,
                    |page: &mut GeneralTab, _window, cx| apply(page, cx),
                    cx,
                )),
        )
        .when(has_changes, |this| {
            this.child(
                h_flex()
                    .w_full()
                    .px(px(4.0))
                    .pb(px(2.0))
                    .flex_wrap()
                    .gap(px(4.0))
                    .children(chips),
            )
        })
        .into_any_element()
}

/// A small label: what an area shows (on) or leaves out (off), or a change being tried.
fn chip(p: &SettingsPalette, text: impl Into<SharedString>, on: bool) -> AnyElement {
    div()
        .flex_shrink_0()
        .h(px(20.0))
        .px(px(7.0))
        .flex()
        .items_center()
        .rounded(px(6.0))
        .text_size(px(11.5))
        .whitespace_nowrap()
        .when(on, |this| {
            this.bg(hsla(p.foreground_alpha(0.08)))
                .text_color(hsla(p.foreground))
        })
        .when(!on, |this| {
            this.border_1()
                .border_color(hsla(p.hairline))
                .text_color(hsla(p.muted))
                .line_through()
        })
        .child(text.into())
        .into_any_element()
}

/// Chill, Regular and Advanced for the whole app, as cards.
fn level_cards(
    page: &GeneralTab,
    p: &SettingsPalette,
    saved: &SettingsValues,
    cx: &mut Context<GeneralTab>,
) -> AnyElement {
    let draft = &page.experience;
    let tried = experience::overall(&|key| draft.value(saved, key));
    let saved_level = experience::overall(&|key| saved.value(key));
    let trying = !draft.changed(saved).is_empty();
    let cards = Preset::ALL.into_iter().map(|preset| {
        let selected = tried == Some(preset);
        let hover = p.raised_hover;
        div()
            .id(SharedString::from(format!(
                "experience-level-{}",
                preset.id()
            )))
            .role(gpui::Role::RadioButton)
            .aria_label(preset.label())
            .aria_toggled(a11y_toggled(selected))
            .flex_1()
            .min_w_0()
            .px(px(14.0))
            .py(px(12.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(hsla(if selected { p.foreground } else { p.hairline }))
            .bg(hsla(p.raised))
            .cursor_pointer()
            .hover(move |this| this.bg(hsla(hover)))
            .on_press(cx, move |page: &mut GeneralTab, _window, cx| {
                try_writes(page, experience::writes_all(preset), cx);
            })
            .child(
                h_flex()
                    .gap(px(8.0))
                    .items_center()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(p.foreground))
                            .child(preset.label()),
                    )
                    .child(div().flex_1())
                    .when(trying && saved_level == Some(preset), |this| {
                        this.child(chip(p, "Saved", true))
                    }),
            )
            .child(
                div()
                    .mt(px(6.0))
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .text_color(hsla(p.muted))
                    .child(preset.description()),
            )
    });
    v_flex()
        .w_full()
        .gap(px(12.0))
        .child(settings_section_heading(
            p,
            "Experience",
            Some("For the whole app.".into()),
            tried.is_none().then(|| custom_note(p)),
        ))
        .child(
            h_flex()
                .w_full()
                .gap(px(10.0))
                .items_stretch()
                .children(cards),
        )
        .into_any_element()
}

/// One area: its name, what its level shows, the levels and Custom, and its switches.
fn area_row(
    page: &GeneralTab,
    p: &SettingsPalette,
    area: &'static Area,
    saved: &SettingsValues,
    cx: &mut Context<GeneralTab>,
) -> AnyElement {
    let draft = &page.experience;
    let value = |key: &str| draft.value(saved, key);
    let level = experience::shown(area, &value);
    let motion = draft.custom.get(area.id);
    let words: SharedString = match level {
        Some(level) => area.words[level as usize].into(),
        None => "Your own mix. Custom shows which switches are on.".into(),
    };
    let chips = area.always.iter().map(|part| chip(p, *part, true)).chain(
        area.controls
            .iter()
            .map(|control| chip(p, control.label, control.shown(&value(control.key)))),
    );
    // The level the area matches stays pressed while its switches are open; Custom is pressed
    // only when it matches none, and a click on Custom opens or closes the switches either way.
    let selected = level.map(Preset::id).unwrap_or("custom");
    let control = settings_segmented_repress(
        p,
        area.id,
        &level_options(),
        Some(selected),
        move |page: &mut GeneralTab, next, _window, cx| match Preset::from_id(&next) {
            Some(preset) => try_writes(page, experience::writes(area, preset), cx),
            None => toggle_custom(page, area.id, cx),
        },
        cx,
    );
    let header = h_flex()
        .w_full()
        .px(px(ROW_PADDING_X))
        .py(px(12.0))
        .gap(px(ROW_GAP))
        .items_center()
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(18.9))
                        .text_color(hsla(p.foreground))
                        .child(area.label),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .text_color(hsla(p.muted))
                        .child(words),
                )
                .child(
                    h_flex()
                        .mt(px(4.0))
                        .flex_wrap()
                        .gap(px(4.0))
                        .children(chips),
                ),
        )
        .child(control);
    let switches = motion.filter(|motion| motion.present()).map(|motion| {
        let rows = area.controls.iter().map(|control| {
            let key = control.key;
            let shown = control.shown(&value(key));
            let row = toggle_field_with(
                p,
                key,
                RowSpec::new(control.label)
                    .description(control.hint)
                    .inset_x(ROW_PADDING_X + 16.0),
                shown,
                None,
                move |page: &mut GeneralTab, next, _window, cx| {
                    let control = experience::area(area.id)
                        .and_then(|area| area.controls.iter().find(|c| c.key == key));
                    if let Some(control) = control {
                        try_writes(page, vec![(key, control.value_for(next))], cx);
                    }
                },
                cx,
            );
            div()
                .w_full()
                .border_t_1()
                .border_color(hsla(p.hairline))
                .child(row)
        });
        let rows: Vec<_> = rows.collect();
        let area_id = area.id;
        render_fold(
            cx.entity(),
            Some(motion),
            move |page: &mut GeneralTab| page.experience.custom.get_mut(area_id),
            v_flex().w_full().children(rows),
        )
    });
    let area_id = area.id;
    v_flex()
        .id(SharedString::from(format!("experience-area-{area_id}")))
        .w_full()
        .on_hover(
            cx.listener(move |page: &mut GeneralTab, hovered: &bool, _, cx| {
                let lit = if *hovered { Some(area_id) } else { None };
                if *hovered || page.experience.hovered == Some(area_id) {
                    page.experience.hovered = lit;
                    cx.notify();
                }
            }),
        )
        .child(header)
        .children(switches)
        .into_any_element()
}
