//! What the GBots card shows under its description while GBots is on: where it runs and what it
//! is doing (downloading with its progress, a failure with Retry, running), the computer it runs
//! on, and, with the Hermes Bots extension on too, which bots the Bots space shows.
//!
//! The status is read from the GBots computer's own gxserver (`/api/gbotsHome status`), through
//! this desktop's connection to it when that is another computer.
//!
//! SEE-ALSO: server/src/gbots/settings.rs (CDXC:GBots 2026-11-07, the one GBots computer), server/src/gbots/download.rs (the download).

use std::time::{Duration, Instant};

use gpui::{AnyElement, Context, IntoElement, ParentElement as _, Styled as _, Window, div, px};
use gpui_component::v_flex;
use serde_json::{Value, json};

use super::super::super::super::native_modal_kit::hsla;
use super::super::super::catalog::SettingOption;
use super::super::super::fields::stock_segmented::stock_segmented;
use super::super::super::fields::{
    SizedButtonSize, SizedButtonVariant, settings_select_with_unavailable, settings_sized_button,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::{SettingsValues, store_machine_gxserver_rpc};
use super::super::machine_picker::machine_choices;
use super::ExtensionsTab;
use super::cards::card_option_row;
use super::data::{is_official_enabled, official_extensions};

const THIS_COMPUTER: &str = "this-computer";
const COMPUTER_KEY: &str = "gbotsComputer";

/// The last status the GBots computer gave, and when to ask again.
#[derive(Default)]
pub(crate) struct GbotsCardState {
    status: Option<Value>,
    error: Option<String>,
    asked_at: Option<Instant>,
    asking: bool,
    /// The computer GBots ran on before the user picked another, named in the card's note.
    left_behind: Option<String>,
}

impl ExtensionsTab {
    /// The card's extra rows while GBots is on.
    pub(crate) fn render_gbots_card_extra(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.ask_gbots_status(values, cx);
        let mut rows: Vec<AnyElement> = Vec::new();
        rows.push(self.render_gbots_status(p, cx));
        if let Some(picker) = self.render_gbots_computer(p, values, window, cx) {
            rows.push(picker);
        }
        if let Some(choice) = self.render_bots_space_choice(p, values, cx) {
            rows.push(choice);
        }
        v_flex().w_full().children(rows).into_any_element()
    }

    fn gbots_machine(values: &SettingsValues) -> Option<String> {
        Some(values.string(COMPUTER_KEY))
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }

    /// Asks the GBots computer for its status, every 2 seconds while something is under way and
    /// every 10 otherwise, and draws the card again when it answers.
    fn ask_gbots_status(&mut self, values: &SettingsValues, cx: &mut Context<Self>) {
        let state = &self.gbots_card;
        let busy = state
            .status
            .as_ref()
            .and_then(|status| status.get("status"))
            .and_then(Value::as_str)
            .is_some_and(|status| matches!(status, "downloading" | "starting"));
        let every = if busy {
            Duration::from_secs(2)
        } else {
            Duration::from_secs(10)
        };
        if state.asking || state.asked_at.is_some_and(|at| at.elapsed() < every) {
            return;
        }
        if self.gbots_card.asked_at.is_none() {
            super::super::machine_picker::request_machine_states(&self.store, cx);
        }
        self.gbots_card.asking = true;
        self.gbots_card.asked_at = Some(Instant::now());
        let machine = Self::gbots_machine(values);
        let tab = cx.entity().downgrade();
        store_machine_gxserver_rpc(
            &self.store,
            machine.as_deref(),
            "/api/gbotsHome",
            json!({ "action": "status" }),
            Duration::from_secs(10),
            move |answer, cx| {
                let _ = tab.update(cx, |tab, cx| {
                    tab.gbots_card.asking = false;
                    match answer {
                        Ok(status) => {
                            tab.gbots_card.status = Some(status);
                            tab.gbots_card.error = None;
                        }
                        Err(error) => tab.gbots_card.error = Some(error),
                    }
                    cx.notify();
                    // Ask again once the wait is over, even while nothing else redraws the page.
                    cx.spawn(async move |tab, cx| {
                        cx.background_executor().timer(every).await;
                        let _ = tab.update(cx, |_, cx| cx.notify());
                    })
                    .detach();
                });
            },
            cx,
        );
    }

    /// One line: what GBots is doing, with Retry after a failed download.
    fn render_gbots_status(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> AnyElement {
        let state = &self.gbots_card;
        let text = |key: &str| {
            state
                .status
                .as_ref()
                .and_then(|status| status.get(key))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let computer = text("computerName").unwrap_or_else(|| "this computer".to_string());
        let download = state
            .status
            .as_ref()
            .and_then(|status| status.get("download"))
            .cloned()
            .unwrap_or(Value::Null);
        let mut retry = false;
        let line = if let Some(error) = &state.error {
            format!("Couldn't reach the GBots computer: {error}")
        } else if state.status.is_none() {
            "Checking GBots…".to_string()
        } else if state
            .status
            .as_ref()
            .and_then(|status| status.get("platformSupported"))
            == Some(&Value::Bool(false))
        {
            "GBots runs on macOS and Linux.".to_string()
        } else if download["phase"] == "failed" {
            retry = true;
            format!(
                "Couldn't download GBots. {}",
                download["error"].as_str().unwrap_or_default()
            )
        } else {
            match text("status").as_deref() {
                Some("running") => format!("Running on {computer}."),
                Some("starting") => format!("Starting on {computer}…"),
                Some("downloading") if download["phase"] == "installing" => {
                    "Installing GBots…".to_string()
                }
                Some("downloading") => {
                    let done = download["doneBytes"].as_f64().unwrap_or(0.0);
                    let total = download["totalBytes"].as_f64().unwrap_or(0.0);
                    if total > 0.0 {
                        format!(
                            "Downloading GBots… {}% of {:.0} MB",
                            ((done / total) * 100.0).floor(),
                            total / 1_000_000.0
                        )
                    } else {
                        "Downloading GBots…".to_string()
                    }
                }
                Some("failed") => {
                    let reason = text("error").unwrap_or_default();
                    format!("GBots stopped on {computer}. {reason} It restarts on its own.")
                }
                Some("notInstalled") if text("componentVersion").is_none() => {
                    "This build of Ghostex has no GBots runtime to download.".to_string()
                }
                Some("notInstalled") => "GBots will download in a moment.".to_string(),
                _ => format!("GBots is off on {computer}."),
            }
        };
        let mut row = div()
            .mt(px(8.0))
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .text_size(px(13.0))
            .line_height(px(18.57))
            .text_color(hsla(p.muted))
            .child(div().flex_1().min_w_0().child(line));
        if retry {
            row = row.child(settings_sized_button(
                p,
                "gbots-download-retry",
                "Retry",
                None,
                None,
                SizedButtonVariant::Outline,
                SizedButtonSize::Sm,
                false,
                None,
                |tab: &mut Self, _window, cx| tab.retry_gbots_download(cx),
                cx,
            ));
        }
        row.into_any_element()
    }

    fn retry_gbots_download(&mut self, cx: &mut Context<Self>) {
        let machine = Self::gbots_machine(&self.store.read(cx).values());
        let tab = cx.entity().downgrade();
        store_machine_gxserver_rpc(
            &self.store,
            machine.as_deref(),
            "/api/gbotsHome",
            json!({ "action": "retryDownload" }),
            Duration::from_secs(10),
            move |_, cx| {
                let _ = tab.update(cx, |tab, cx| {
                    tab.gbots_card.asked_at = None;
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// "Runs on": this computer or one of the saved remote machines; only while there is a choice.
    fn render_gbots_computer(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let store = self.store.read(cx);
        let choices = machine_choices(
            &store.value("remoteMachines"),
            store.host_payload("remoteMachineConnectStates"),
        );
        if choices.len() < 2 {
            return None;
        }
        let current = Self::gbots_machine(values);
        let options: Vec<SettingOption> = choices
            .iter()
            .map(|choice| SettingOption {
                label: match &choice.unavailable {
                    Some(reason) => format!("{} · {reason}", choice.name),
                    None => choice.name.clone(),
                },
                value: choice
                    .machine_id
                    .clone()
                    .unwrap_or_else(|| THIS_COMPUTER.to_string()),
            })
            .collect();
        let unavailable: Vec<String> = choices
            .iter()
            .filter(|choice| choice.unavailable.is_some() && choice.machine_id != current)
            .filter_map(|choice| choice.machine_id.clone())
            .collect();
        let value = current.clone().unwrap_or_else(|| THIS_COMPUTER.to_string());
        let names: Vec<(Option<String>, String)> = choices
            .iter()
            .map(|choice| (choice.machine_id.clone(), choice.name.clone()))
            .collect();
        let select = settings_select_with_unavailable(
            self,
            p,
            "gbots-computer",
            &options,
            &unavailable,
            &value,
            Some(220.0),
            false,
            None,
            move |tab: &mut Self, picked: String, _window, cx| {
                let picked = (picked != THIS_COMPUTER).then_some(picked);
                let name_of = |id: &Option<String>| {
                    names
                        .iter()
                        .find(|(machine, _)| machine == id)
                        .map(|(_, name)| name.clone())
                        .unwrap_or_else(|| "this computer".to_string())
                };
                tab.move_gbots_computer(picked, name_of, cx);
            },
            window,
            cx,
        );
        let mut column = v_flex()
            .w_full()
            .child(card_option_row(p, "Runs on", select));
        let note = match &self.gbots_card.left_behind {
            Some(old) => format!(
                "Bots made on {old} stay there until moving them is available. Accounts for the bots are the ones saved on the GBots computer."
            ),
            None => "Bots, their conversations and the Claude and Codex accounts they use live on the GBots computer.".to_string(),
        };
        column = column.child(
            div()
                .mt(px(4.0))
                .text_size(px(12.0))
                .line_height(px(17.0))
                .text_color(hsla(p.muted))
                .child(note),
        );
        Some(column.into_any_element())
    }

    /// Makes `picked` (`None`: this computer) the GBots computer: the new one is told first, then
    /// the setting changes and the old remote one is released.
    fn move_gbots_computer(
        &mut self,
        picked: Option<String>,
        name_of: impl Fn(&Option<String>) -> String,
        cx: &mut Context<Self>,
    ) {
        let current = Self::gbots_machine(&self.store.read(cx).values());
        if current == picked {
            return;
        }
        let old_name = name_of(&current);
        let store = self.store.clone();
        let tab = cx.entity().downgrade();
        let finish = move |picked: Option<String>, current: Option<String>, cx: &mut gpui::App| {
            store.update(cx, |store, cx| {
                store.update_setting(COMPUTER_KEY, json!(picked.clone().unwrap_or_default()), cx)
            });
            if let Some(old) = current {
                store_machine_gxserver_rpc(
                    &store,
                    Some(old.as_str()),
                    "/api/gbotsHome",
                    json!({ "action": "release" }),
                    Duration::from_secs(10),
                    |_, _| {},
                    cx,
                );
            }
            let _ = tab.update(cx, |tab, cx| {
                tab.gbots_card.left_behind = Some(old_name.clone());
                tab.gbots_card.status = None;
                tab.gbots_card.asked_at = None;
                cx.notify();
            });
        };
        match picked.clone() {
            Some(machine) => {
                let tab = cx.entity().downgrade();
                store_machine_gxserver_rpc(
                    &self.store,
                    Some(machine.as_str()),
                    "/api/gbotsHome",
                    json!({ "action": "claim", "by": crate::host_name::local_computer_name() }),
                    Duration::from_secs(10),
                    move |answer, cx| match answer {
                        Ok(_) => finish(picked, current, cx),
                        Err(error) => {
                            let _ = tab.update(cx, |tab, cx| {
                                tab.gbots_card.error = Some(error);
                                cx.notify();
                            });
                        }
                    },
                    cx,
                );
            }
            None => finish(None, current, cx),
        }
    }

    /// The GBots card's "Bots space shows" pick, while the Hermes Bots extension is on too: with
    /// only one of them on the space shows that one (CDXC:GBots 2026-11-06 in gx-core
    /// `BotsSource`), so the pick exists only when there is a choice.
    pub(crate) fn render_bots_space_choice(
        &self,
        p: &SettingsPalette,
        values: &SettingsValues,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let hermes = official_extensions()
            .iter()
            .find(|extension| extension.id == "bots")?;
        if !is_official_enabled(values, hermes) {
            return None;
        }
        let options = vec![
            ("hermes".to_string(), "Hermes".to_string()),
            ("gbots".to_string(), "GBots".to_string()),
        ];
        let selected = values.choice(
            "botsSpaceSource",
            &["hermes".to_string(), "gbots".to_string()],
        );
        let control = stock_segmented(
            p,
            "gbots-bots-space-source",
            &options,
            Some(selected.as_str()),
            false,
            false,
            |page: &mut Self, next, _window, cx| {
                let store = page.store.clone();
                store.update(cx, |store, cx| {
                    store.update_setting("botsSpaceSource", json!(next), cx)
                });
            },
            cx,
        );
        Some(card_option_row(p, "Bots space shows", control))
    }
}
