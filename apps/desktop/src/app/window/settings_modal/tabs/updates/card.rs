//! One Updates card: the logo, name and what it is, one status word (Failed with a red dot), the
//! installed and newest versions, a line on what the update does or why it failed, and the footer
//! with the "Auto-update" switch and the card's one action.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    ButtonSize, ButtonVariant, ROW_PADDING_X, full_value_tooltip, settings_button_sized,
    settings_icon, short_version, small_switch_control,
};
use super::super::super::palette::SettingsPalette;
use super::super::agents::logos::{agent_logo, muted_fill};
use super::super::extensions::cards::CARD_MIN_HEIGHT;
use super::UpdatesTab;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    Animation, AnimationExt as _, AnyElement, Context, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    Transformation, div, img, px, radians,
};
use gpui_component::{h_flex, v_flex};
use serde_json::Value;
use std::time::Duration;

fn text<'a>(item: &'a Value, key: &str) -> Option<&'a str> {
    item[key].as_str().filter(|text| !text.is_empty())
}

/// The item's logo: an agent's brand logo (Claude Swap and Codex Swap use Claude's and Codex's),
/// the app icon for Ghostex, else a Settings glyph.
pub(super) fn item_icon(p: &SettingsPalette, item: &Value, size: f32) -> AnyElement {
    let icon = text(item, "icon").unwrap_or_default();
    let brand = match icon {
        "claude-swap" => "claude",
        "codex-swap" => "codex",
        other => other,
    };
    if let Some(logo) = agent_logo(brand, size, p) {
        return logo;
    }
    if icon == "ghostex" {
        return img("app-icon.png")
            .size(px(size + 2.0))
            .flex_shrink_0()
            .into_any_element();
    }
    let glyph = match icon {
        "machine" => "modals/settings/device-desktop.svg",
        "trycua" => "modals/settings/device-desktop.svg",
        "spaceo" => "modals/settings/device-laptop.svg",
        "code" => "titlebar/brand-vscode.svg",
        "cef" => "modals/settings/browser.svg",
        "collab" => "modals/settings/users.svg",
        "easyConnect" => "modals/settings/plug-connected.svg",
        "extension" => "modals/settings/puzzle.svg",
        "node" => "modals/settings/brand-nodejs.svg",
        "uv" => "modals/settings/brand-python.svg",
        "gh" => "modals/settings/brand-github.svg",
        "glab" => "modals/settings/brand-gitlab.svg",
        "hex" => "modals/settings/microphone.svg",
        _ => "modals/settings/tools.svg",
    };
    settings_icon(glyph, size, p.foreground)
        .flex_shrink_0()
        .into_any_element()
}

/// The cards' logo tile: the item's logo on Settings' muted fill.
fn logo_tile(p: &SettingsPalette, item: &Value, tile: f32, logo: f32) -> AnyElement {
    div()
        .flex_shrink_0()
        .size(px(tile))
        .rounded(px(tile * 0.27))
        .bg(hsla(muted_fill(p)))
        .flex()
        .items_center()
        .justify_center()
        .child(item_icon(p, item, logo))
        .into_any_element()
}

/// The pills' shared shape: logo, then name, at one height and padding.
fn pill_base(p: &SettingsPalette, item: &Value, id: String) -> gpui::Stateful<gpui::Div> {
    h_flex()
        .id(SharedString::from(id))
        .flex_shrink_0()
        .items_center()
        .gap(px(6.0))
        .h(px(PILL_HEIGHT))
        .pl(px(8.0))
        .pr(px(11.0))
        .rounded_full()
        .text_size(px(12.0))
        .line_height(px(16.0))
        .child(item_icon(p, item, 13.0))
}

/// A small turning loader (an install or update that runs).
pub(super) fn spinner(p: &SettingsPalette, id: &str, size: f32) -> AnyElement {
    settings_icon("modals/settings/loader-2.svg", size, p.muted)
        .flex_shrink_0()
        .with_animation(
            SharedString::from(format!("{id}-spin")),
            Animation::new(Duration::from_millis(1000))
                .repeat()
                .with_max_fps(crate::app::helpers::INDICATOR_MAX_FPS),
            |icon, delta| {
                icon.with_transformation(Transformation::rotate(radians(
                    delta * std::f32::consts::TAU,
                )))
            },
        )
        .into_any_element()
}

/// The pills' height.
const PILL_HEIGHT: f32 = 28.0;

/// The version a pill or card shows, and the whole value when that was shortened (its tooltip).
/// A build id or hash is never shown whole (`long_value.rs`).
pub(super) fn shown_version(version: &str) -> (Option<String>, Option<String>) {
    let short = short_version(version);
    let full = (short.as_deref() != Some(version)).then(|| version.to_string());
    (short, full)
}

/// One Up to date pill: logo, name, and the version in the muted tone; a version too long to show
/// (a build id) is cut to its number, or left out, and is whole on hover.
pub(super) fn up_to_date_pill(p: &SettingsPalette, item: &Value) -> AnyElement {
    let name = text(item, "name").unwrap_or_default().to_string();
    let (version, full) = text(item, "installed")
        .map(shown_version)
        .unwrap_or((None, None));
    let pill = pill_base(
        p,
        item,
        format!("updates-pill-{}", text(item, "id").unwrap_or_default()),
    )
    .bg(hsla(muted_fill(p)))
    .child(
        div()
            .whitespace_nowrap()
            .font_weight(FontWeight::MEDIUM)
            .text_color(hsla(p.foreground_alpha(0.88)))
            .child(name.clone()),
    )
    .children(version.map(|version| {
        div()
            .whitespace_nowrap()
            .text_color(hsla(p.muted))
            .child(version)
    }));
    match full {
        Some(full) => full_value_tooltip(pill, format!("{name} {full}")).into_any_element(),
        None => pill.into_any_element(),
    }
}

/// One Not installed pill: logo and name. The pill is the button: hover shows it can be pressed,
/// a click installs it on its computer, and while that runs it shows a spinner, then moves to
/// where its new state belongs.
///
/// CDXC:Updates 2026-11-09 DECISION:
/// User: "please don't show Install next to each pill for something I can install in the update
/// page I dont like that". No "Install" word; the tooltip says "Install <name>".
pub(super) fn install_pill(
    p: &SettingsPalette,
    item: &Value,
    cx: &mut Context<UpdatesTab>,
) -> AnyElement {
    let id = text(item, "id").unwrap_or_default().to_string();
    let name = text(item, "name").unwrap_or_default().to_string();
    let installing = super::installing(item);
    let pill = pill_base(p, item, format!("updates-install-{id}"))
        .border_1()
        .border_color(hsla(p.hairline))
        .child(
            div()
                .whitespace_nowrap()
                .font_weight(FontWeight::MEDIUM)
                .text_color(hsla(p.foreground_alpha(0.78)))
                .child(name.clone()),
        );
    if installing {
        let step = text(item, "progress").unwrap_or("Installing…").to_string();
        return full_value_tooltip(
            pill.child(spinner(p, &format!("updates-installing-{id}"), 12.0)),
            format!("{name}: {step}"),
        )
        .into_any_element();
    }
    let hover = p.foreground_alpha(0.08);
    let tooltip = match text(item, "note") {
        Some(note) => format!("Install {name}\n{note}"),
        None => format!("Install {name}"),
    };
    full_value_tooltip(
        pill.cursor_pointer()
            .hover(move |this| this.bg(hsla(hover))),
        tooltip,
    )
    .on_click(cx.listener(move |tab, _, _, cx| tab.install(&id, cx)))
    .into_any_element()
}

/// A row under the pills (Checking, or a version Ghostex can't read), laid out like a card's title.
pub(super) fn up_to_date_row(
    p: &SettingsPalette,
    item: &Value,
    detail: String,
    control: Option<AnyElement>,
) -> AnyElement {
    h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap(px(12.0))
        .px(px(ROW_PADDING_X))
        .py(px(12.0))
        .child(logo_tile(p, item, 30.0, 16.0))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(14.0))
                        .line_height(px(19.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(hsla(p.foreground))
                        .child(text(item, "name").unwrap_or_default().to_string()),
                )
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(hsla(p.muted))
                        .child(detail),
                ),
        )
        .children(control)
        .into_any_element()
}

/// The status word beside the name; `None` for a plain update (its button says it).
fn status_word(item: &Value) -> Option<&'static str> {
    match item["state"].as_str() {
        Some("failed") => Some("Failed"),
        Some("needsYou") => Some("Needs you"),
        Some("updating") => Some("Updating"),
        Some("available") if item["major"] == true => Some("Major"),
        _ => None,
    }
}

impl UpdatesTab {
    pub(super) fn card(
        &self,
        p: &SettingsPalette,
        item: &Value,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = text(item, "id").unwrap_or_default().to_string();
        let state = text(item, "state").unwrap_or_default();
        let name = text(item, "name").unwrap_or_default().to_string();
        let failed = state == "failed";
        let updating = state == "updating";

        let title = h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap(px(10.0))
            .child(logo_tile(p, item, 30.0, 16.0))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(14.0))
                            .line_height(px(19.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(p.foreground))
                            .child(name.clone()),
                    )
                    .children(text(item, "detail").map(|detail| {
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(hsla(p.muted))
                            .child(detail.to_string())
                    })),
            )
            .children(status_word(item).map(|word| {
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(5.0))
                    .when(failed, |this| {
                        this.child(div().size(px(6.0)).rounded_full().bg(hsla(p.destructive)))
                    })
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(hsla(p.muted))
                    .child(word)
            }));

        // A build id or hash shows as its version number, or not at all (`long_value.rs`).
        let installed_full = text(item, "installed");
        let latest_full = text(item, "latest");
        let installed_short = installed_full.and_then(short_version);
        let latest_short = latest_full.and_then(short_version);
        let shortened =
            installed_short.as_deref() != installed_full || latest_short.as_deref() != latest_full;
        // The whole values, on hover, when either was shortened.
        let full_versions = shortened.then(|| match (installed_full, latest_full) {
            (Some(installed), Some(latest)) if installed != latest => {
                format!("{installed} → {latest}")
            }
            (Some(version), _) | (None, Some(version)) => version.to_string(),
            (None, None) => String::new(),
        });
        let installed = installed_short.as_deref();
        let latest = latest_short.as_deref();
        let latest = if installed_full != latest_full && installed == latest {
            // Two builds of one version: still an update, say so without the ids.
            None
        } else {
            latest
        };
        let version = match (installed, latest, text(item, "summary")) {
            (_, _, Some(summary)) if latest.is_none() => Some(
                h_flex()
                    .gap(px(4.0))
                    .text_color(hsla(p.muted))
                    .children(installed.map(|installed| format!("{installed} ·")))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_color(hsla(p.foreground))
                            .child(summary.to_string()),
                    )
                    .into_any_element(),
            ),
            (Some(installed), Some(latest), _) if installed != latest => Some(
                h_flex()
                    .gap(px(5.0))
                    .text_color(hsla(p.muted))
                    .child(installed.to_string())
                    .child("→")
                    .child(
                        div()
                            .text_color(hsla(p.foreground))
                            .font_weight(FontWeight::MEDIUM)
                            .child(latest.to_string()),
                    )
                    .into_any_element(),
            ),
            (Some(installed), _, _) | (None, Some(installed), _) => Some(
                div()
                    .text_color(hsla(p.muted))
                    .child(installed.to_string())
                    .into_any_element(),
            ),
            _ => None,
        };

        let line = if updating {
            text(item, "progress").or(Some("Updating…"))
        } else {
            text(item, "error").or(text(item, "note"))
        };

        let action: Option<AnyElement> = if updating {
            Some(
                h_flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(12.0))
                    .text_color(hsla(p.muted))
                    .child(spinner(p, &format!("updates-updating-{id}"), 13.0))
                    .child("Updating")
                    .into_any_element(),
            )
        } else if state == "needsYou" {
            let review = item.clone();
            Some(settings_button_sized(
                p,
                SharedString::from(format!("updates-review-{id}")),
                "Review…",
                None,
                ButtonVariant::Default,
                ButtonSize::Xs,
                false,
                None,
                move |tab: &mut Self, _window, cx| tab.review(&review, cx),
                cx,
            ))
        } else {
            let install = id.clone();
            let can_update = item["canUpdate"] == true;
            let update = settings_button_sized(
                p,
                SharedString::from(format!("updates-update-{id}")),
                if failed { "Retry" } else { "Update" },
                None,
                ButtonVariant::Default,
                ButtonSize::Xs,
                !can_update,
                Some(SharedString::from(
                    text(item, "note")
                        .unwrap_or("Ghostex can't run this update from here.")
                        .to_string(),
                )),
                move |tab: &mut Self, _window, cx| tab.install(&install, cx),
                cx,
            );
            // Dismiss clears the failure here and in the top bar's Updates list (its red button)
            // until this item's next update (titlebar/updates_menu_rows.rs).
            Some(if failed {
                let (dismiss, error) = (
                    id.clone(),
                    text(item, "error").unwrap_or_default().to_string(),
                );
                h_flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(settings_button_sized(
                        p,
                        SharedString::from(format!("updates-dismiss-{id}")),
                        "Dismiss",
                        None,
                        ButtonVariant::Outline,
                        ButtonSize::Xs,
                        false,
                        None,
                        move |tab: &mut Self, _window, cx| tab.dismiss(&dismiss, &error, cx),
                        cx,
                    ))
                    .child(update)
                    .into_any_element()
            } else {
                update
            })
        };

        let auto = (item["canAuto"] == true).then(|| {
            let toggle = id.clone();
            let checked = self.auto(item);
            let auto_switch = small_switch_control(
                p,
                SharedString::from(format!("updates-auto-{id}")),
                format!("Update {name} automatically"),
                checked,
                false,
                None,
                move |tab: &mut Self, enabled, _window, cx| tab.set_auto(&toggle, enabled, cx),
                cx,
            );
            let auto_press = auto_switch.press();
            h_flex()
                .id(SharedString::from(format!("updates-auto-row-{id}")))
                .min_w_0()
                .items_center()
                .gap(px(6.0))
                .when_some(auto_press, |row, press| {
                    row.switch_row_press(switch_row_wash(p.foreground), false, press)
                })
                .child(auto_switch)
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(12.0))
                        .text_color(hsla(p.muted))
                        .child("Auto-update"),
                )
        });

        v_flex()
            .w_full()
            .min_w_0()
            .min_h(px(CARD_MIN_HEIGHT))
            .pt(px(12.0))
            .px(px(12.0))
            .pb(px(10.0))
            .gap(px(8.0))
            .rounded(px(MODAL_RADIUS_SECTION))
            .border_1()
            .border_color(hsla(p.hairline))
            .bg(hsla(p.raised))
            .child(title)
            .children(version.map(|version| {
                let row = div()
                    .id(SharedString::from(format!("updates-version-{id}")))
                    .min_w_0()
                    .font_family(MODAL_MONO_FONT)
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .child(version);
                match full_versions.clone() {
                    Some(full) => full_value_tooltip(row, full).into_any_element(),
                    None => row.into_any_element(),
                }
            }))
            .children(line.map(|line| {
                div()
                    .min_w_0()
                    .line_clamp(2)
                    .text_ellipsis()
                    .text_size(px(12.0))
                    .line_height(px(16.5))
                    .text_color(hsla(p.foreground_alpha(0.7)))
                    .child(line.to_string())
            }))
            .child(
                h_flex()
                    .mt_auto()
                    .w_full()
                    .min_h(px(26.0))
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(div().min_w_0().children(auto))
                    .children(action),
            )
            .into_any_element()
    }
}
