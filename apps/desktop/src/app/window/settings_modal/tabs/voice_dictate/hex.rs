//! Settings › Dictation's Hex row, shown when the engine is Hex: where the user stands with
//! Hex 3 on the shown computer (`hex` in `/api/readDictateSettings`), in plain words. Hex 3 not
//! found gets Install Hex (Ghostex's install, `/api/managedTools`, only on click) and Hex's
//! download page; Hex not running gets Check again; a running Hex gets a picker of the models it
//! has ready.
//! SEE-ALSO: server/src/voice_agent/speech/hex.rs (CDXC:Dictate 2026-11-07), server/src/managed_tools/hex.rs.

use super::*;

/// How often the row re-reads the install job while Hex installs.
const INSTALL_POLL: Duration = Duration::from_secs(2);

/// What to do in Hex after the step the row shows, so setting it up reads as a short list.
///
/// CDXC:Dictate 2026-11-08 DECISION: User (Q1709): with our local engine gone, Settings explains how to use Hex: install it, open it and allow the microphone, download a model in Hex's settings, and keep it opening at login so dictation works after a restart.
pub(super) fn hex_next_step(state: &str) -> Option<&'static str> {
    match state {
        "notInstalled" => Some(
            "After it installs: open Hex, allow the microphone, and download a model in Hex's settings (Parakeet v3 for English and 24 European languages, Whisper for the rest). Ghostex finds it by itself.",
        ),
        "notRunning" => Some(
            "Open Hex, then Check again. Turn on Open at login in Hex so dictation keeps working after a restart.",
        ),
        "ready" | "problem" => Some(
            "In Hex's settings, under Models, download a model; Ghostex lists it here once it is ready.",
        ),
        _ => None,
    }
}

impl VoiceAgentTab {
    /// Hex is offered where Hex 3 ships for the shown computer.
    pub(super) fn hex_offered(answer: &Value) -> bool {
        answer["hex"]["available"] == true
    }

    pub(super) fn hex_row(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let answer = self.dictate.answer.clone()?;
        if answer["settings"]["engine"] != "hex" || !Self::hex_offered(&answer) {
            return None;
        }
        let hex = &answer["hex"];
        let state = hex["state"].as_str().unwrap_or("notRunning");
        let installing = self.dictate.hex_install.is_some();
        let description = match hex["problem"]["message"].as_str() {
            _ if installing => "Installing Hex…".to_string(),
            Some(message) => match hex_next_step(state) {
                Some(step) => format!("{message}\n{step}"),
                None => message.to_string(),
            },
            None => format!(
                "Hex {} is running. Recordings stay on this computer.",
                hex["version"].as_str().unwrap_or_default()
            ),
        };
        let check_again = settings_button(
            p,
            "dictate-hex-check",
            "Check again",
            None,
            ButtonVariant::Outline,
            false,
            None,
            |page: &mut Self, _window, cx| page.refresh_dictate_quietly(cx),
            cx,
        );
        let control = match state {
            "notInstalled" => {
                let install_label = if hex["olderVersion"].is_string() {
                    "Install Hex 3"
                } else {
                    "Install Hex"
                };
                let url = hex["downloadUrl"].as_str().unwrap_or_default().to_string();
                h_flex()
                    .gap(px(8.0))
                    .items_center()
                    .child(settings_button(
                        p,
                        "dictate-hex-install",
                        if installing {
                            "Installing…"
                        } else {
                            install_label
                        },
                        None,
                        ButtonVariant::Default,
                        installing,
                        None,
                        |page: &mut Self, _window, cx| page.install_hex(cx),
                        cx,
                    ))
                    .child(settings_button(
                        p,
                        "dictate-hex-download",
                        "Download page",
                        None,
                        ButtonVariant::Outline,
                        false,
                        None,
                        move |page: &mut Self, _window, cx| {
                            post_store_message(
                                &page.store,
                                json!({ "type": "openExternalUrl", "url": url.clone() }),
                                cx,
                            );
                        },
                        cx,
                    ))
                    .into_any_element()
            }
            "ready" => {
                let options: Vec<SettingOption> = hex["models"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|model| {
                        Some(SettingOption {
                            label: model["name"].as_str()?.to_string(),
                            value: model["id"].as_str()?.to_string(),
                        })
                    })
                    .collect();
                if options.is_empty() {
                    check_again
                } else {
                    let value = hex["model"]
                        .as_str()
                        .or(answer["settings"]["hexModel"].as_str())
                        .unwrap_or_default()
                        .to_string();
                    h_flex()
                        .gap(px(8.0))
                        .items_center()
                        .child(settings_select(
                            self,
                            p,
                            "dictate-hex-model",
                            &options,
                            &value,
                            None,
                            false,
                            None,
                            |page: &mut Self, value, _window, cx| {
                                page.set_dictate(json!({ "hexModel": value }), cx)
                            },
                            window,
                            cx,
                        ))
                        .child(check_again)
                        .into_any_element()
                }
            }
            _ => check_again,
        };
        // Ready: the model picker on the right. Otherwise what is missing and what to do next, in
        // full under the label, with its buttons (rows show no description text).
        if state == "ready" && hex["ready"] == true {
            return Some(setting_row(
                p,
                "dictateHex",
                RowSpec::new("Hex").description(description),
                None,
                control,
                cx,
            ));
        }
        Some(setting_row(
            p,
            "dictateHex",
            RowSpec::new("Hex").wide(),
            None,
            super::notice::note_with_controls(p, description, vec![control]),
            cx,
        ))
    }

    /// Install Hex: Ghostex's install of Hex on the shown computer, followed until it ends.
    pub(super) fn install_hex(&mut self, cx: &mut Context<Self>) {
        if self.dictate.hex_install.is_some() {
            return;
        }
        self.dictate.action_error = None;
        let machine = self.picked_machine(cx);
        let (sender, receiver) = futures::channel::oneshot::channel();
        store_machine_gxserver_rpc(
            &self.store,
            machine.as_deref(),
            "/api/managedTools",
            json!({ "action": "start", "tool": "hex", "operation": "install" }),
            READ_TIMEOUT,
            move |result, _| {
                let _ = sender.send(result);
            },
            cx,
        );
        self.dictate.hex_install = Some(cx.spawn(async move |page, cx| {
            let started = receiver
                .await
                .unwrap_or_else(|_| Err("The install could not start.".into()));
            let mut failure = started.err();
            while failure.is_none() {
                cx.background_executor().timer(INSTALL_POLL).await;
                let asked = page.update(cx, |page, cx| {
                    let (sender, receiver) = futures::channel::oneshot::channel();
                    store_machine_gxserver_rpc(
                        &page.store,
                        machine.as_deref(),
                        "/api/managedTools",
                        json!({ "action": "read", "tool": "hex" }),
                        READ_TIMEOUT,
                        move |result, _| {
                            let _ = sender.send(result);
                        },
                        cx,
                    );
                    receiver
                });
                let Ok(receiver) = asked else {
                    return;
                };
                // A dropped answer is asked again on the next tick.
                let Ok(Ok(tool)) = receiver.await else {
                    continue;
                };
                match tool["job"]["status"].as_str() {
                    Some("queued" | "running") => continue,
                    Some("failed") => {
                        failure = Some(
                            tool["job"]["error"]
                                .as_str()
                                .unwrap_or("Hex's install failed.")
                                .to_string(),
                        )
                    }
                    _ => break,
                }
            }
            let _ = page.update(cx, |page, cx| {
                page.dictate.hex_install = None;
                if let Some(error) = failure {
                    page.dictate.action_error = Some(format!("Hex wasn't installed: {error}"));
                }
                page.refresh_dictate_quietly(cx);
                cx.notify();
            });
        }));
        cx.notify();
    }
}
