//! Settings > Updates > Go back: the version this computer ran before, any time (not only after a
//! crash), with a short confirm, and where any older version can be downloaded.
//!
//! CDXC:Release 2026-11-17 DECISION:
//! User: "we need to work on being able to go back to older version from settings -> updates
//! without crashes (just because something is buggy etc)". The app sends the offer in
//! `updatesState.goBack` (app/updates/mod.rs, from go_back/mod.rs `offer`); Go back posts
//! `{type: "updates", op: "goBack"}` and the app swaps the kept version in and restarts. A deb or
//! rpm install gets the command that installs the older package; a computer with no kept copy gets
//! the GitHub download. The section shows for this computer only.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    ButtonSize, ButtonVariant, settings_button_sized, settings_list_item_wrapped, settings_section,
};
use super::super::super::palette::SettingsPalette;
use super::UpdatesTab;
use gpui::{
    AnyElement, ClipboardItem, Context, IntoElement, ParentElement as _, SharedString, Styled as _,
    div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};

/// Where every older version can be downloaded.
pub(super) const RELEASES_URL: &str = "https://github.com/maddada/Ghostex/releases";

impl UpdatesTab {
    pub(super) fn go_back_section(
        &mut self,
        p: &SettingsPalette,
        payload: Option<&Value>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let mut rows: Vec<AnyElement> = Vec::new();
        if let Some(offer) = payload
            .map(|payload| &payload["goBack"])
            .filter(|offer| offer.is_object())
        {
            rows.push(self.go_back_row(p, offer, cx));
        }
        rows.push(settings_list_item_wrapped(
            p,
            div().child("Older versions"),
            muted(
                p,
                "Every earlier version of Ghostex can be downloaded from GitHub.",
            ),
            Some(settings_button_sized(
                p,
                "updates-go-back-releases",
                "Open GitHub",
                None,
                ButtonVariant::Default,
                ButtonSize::Sm,
                false,
                None,
                |_: &mut Self, _, cx| cx.open_url(RELEASES_URL),
                cx,
            )),
        ));
        settings_section(p, "Go back", None, None, rows).map(IntoElement::into_any_element)
    }

    fn go_back_row(
        &mut self,
        p: &SettingsPalette,
        offer: &Value,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = offer["current"].as_str().unwrap_or_default().to_string();
        let previous = offer["previous"].as_str().unwrap_or_default().to_string();
        let title = div().child(format!("Go back to {previous}"));
        match offer["how"].as_str() {
            Some("command") => {
                let command = offer["command"].as_str().unwrap_or_default().to_string();
                let detail = v_flex()
                    .gap(px(6.0))
                    .child(muted(
                        p,
                        "Run this in a terminal, then open Ghostex again. Your chats, projects and settings stay as they are.",
                    ))
                    .child(
                        div()
                            .px(px(8.0))
                            .py(px(6.0))
                            .rounded(px(6.0))
                            .bg(hsla(p.raised))
                            .font_family(MODAL_MONO_FONT)
                            .text_size(px(12.0))
                            .line_height(px(18.0))
                            .child(command.clone()),
                    )
                    .into_any_element();
                let copied = self.go_back_copied;
                let copy = settings_button_sized(
                    p,
                    "updates-go-back-copy",
                    if copied { "Copied" } else { "Copy command" },
                    None,
                    ButtonVariant::Outline,
                    ButtonSize::Sm,
                    false,
                    None,
                    move |tab: &mut Self, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(command.clone()));
                        tab.go_back_copied = true;
                        cx.notify();
                    },
                    cx,
                );
                // Run in terminal: a Main Hub terminal with the same command typed in, which
                // gxserver works out again from the version (`linuxGoBack`, server/src/shown_commands/).
                let version = previous.clone();
                let run = settings_button_sized(
                    p,
                    "updates-go-back-run-in-terminal",
                    "Run in terminal",
                    None,
                    ButtonVariant::Default,
                    ButtonSize::Sm,
                    false,
                    None,
                    move |tab: &mut Self, _, cx| {
                        tab.post(
                            json!({
                                "type": "openShownCommandTerminal",
                                "commandId": "linuxGoBack",
                                "args": { "version": version },
                            }),
                            cx,
                        );
                    },
                    cx,
                );
                settings_list_item_wrapped(
                    p,
                    title,
                    detail,
                    Some(
                        h_flex()
                            .gap(px(6.0))
                            .child(copy)
                            .child(run)
                            .into_any_element(),
                    ),
                )
            }
            Some("download") => {
                let url = offer["url"].as_str().unwrap_or(RELEASES_URL).to_string();
                settings_list_item_wrapped(
                    p,
                    title,
                    muted(
                        p,
                        format!(
                            "Download {previous} from GitHub and install it over {current}. Your chats, projects and settings stay as they are."
                        ),
                    ),
                    Some(settings_button_sized(
                        p,
                        "updates-go-back-download",
                        format!("Download {previous}"),
                        None,
                        ButtonVariant::Default,
                        ButtonSize::Sm,
                        false,
                        None,
                        move |_: &mut Self, _, cx| cx.open_url(&url),
                        cx,
                    )),
                )
            }
            _ if self.confirm_go_back => {
                let cancel = settings_button_sized(
                    p,
                    "updates-go-back-cancel",
                    "Cancel",
                    None,
                    ButtonVariant::Default,
                    ButtonSize::Sm,
                    false,
                    None,
                    |tab: &mut Self, _, cx| {
                        tab.confirm_go_back = false;
                        cx.notify();
                    },
                    cx,
                );
                let confirm = settings_button_sized(
                    p,
                    "updates-go-back-confirm",
                    "Go back",
                    None,
                    ButtonVariant::Default,
                    ButtonSize::Sm,
                    false,
                    None,
                    |tab: &mut Self, _, cx: &mut Context<Self>| {
                        tab.confirm_go_back = false;
                        tab.post(json!({ "type": "updates", "op": "goBack" }), cx);
                        cx.notify();
                    },
                    cx,
                );
                settings_list_item_wrapped(
                    p,
                    div().child(format!("Go back to {previous}?")),
                    muted(
                        p,
                        format!(
                            "Ghostex closes and opens again on {previous}. It won't offer {current} again until a newer version is out."
                        ),
                    ),
                    Some(
                        h_flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(cancel)
                            .child(confirm)
                            .into_any_element(),
                    ),
                )
            }
            _ => settings_list_item_wrapped(
                p,
                title,
                muted(
                    p,
                    format!(
                        "The version you had before {current}, if something isn't working right. Your chats, projects and settings stay as they are."
                    ),
                ),
                Some(settings_button_sized(
                    p,
                    "updates-go-back",
                    "Go back…",
                    None,
                    ButtonVariant::Default,
                    ButtonSize::Sm,
                    false,
                    None,
                    |tab: &mut Self, _, cx| {
                        tab.confirm_go_back = true;
                        cx.notify();
                    },
                    cx,
                )),
            ),
        }
    }
}

fn muted(p: &SettingsPalette, text: impl Into<SharedString>) -> AnyElement {
    div()
        .whitespace_normal()
        .text_color(hsla(p.muted))
        .child(text.into())
        .into_any_element()
}
