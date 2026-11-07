//! One Updates card: the logo, name and what it is, one status word (Failed with a red dot), the
//! installed and newest versions, a line on what the update does or why it failed, and the footer
//! with the "Auto-update" switch and the card's one action.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    ButtonSize, ButtonVariant, settings_button_sized, settings_icon, small_switch_control,
};
use super::super::super::palette::SettingsPalette;
use super::super::agents::logos::{agent_logo, muted_fill};
use super::super::extensions::cards::CARD_MIN_HEIGHT;
use super::UpdatesTab;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, Styled as _, div, img, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::Value;

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

/// One Up to date pill: logo, name, and the version in the muted tone.
pub(super) fn up_to_date_pill(p: &SettingsPalette, item: &Value) -> AnyElement {
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.0))
        .h(px(26.0))
        .pl(px(7.0))
        .pr(px(10.0))
        .rounded_full()
        .bg(hsla(muted_fill(p)))
        .text_size(px(12.0))
        .line_height(px(16.0))
        .child(item_icon(p, item, 13.0))
        .child(
            div()
                .whitespace_nowrap()
                .font_weight(FontWeight::MEDIUM)
                .text_color(hsla(p.foreground_alpha(0.88)))
                .child(text(item, "name").unwrap_or_default().to_string()),
        )
        .children(text(item, "installed").map(|version| {
            div()
                .whitespace_nowrap()
                .text_color(hsla(p.muted))
                .child(version.to_string())
        }))
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
        .gap(px(10.0))
        .px(px(14.0))
        .py(px(11.0))
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

        let installed = text(item, "installed");
        let latest = text(item, "latest");
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
                    .child(settings_icon("modals/settings/loader-2.svg", 13.0, p.muted))
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
                div()
                    .min_w_0()
                    .font_family(MODAL_MONO_FONT)
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .child(version)
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
