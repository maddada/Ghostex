//! Settings › Dictation's Hex row, shown when the engine is Hex: where the user stands with
//! Hex 3 on the shown computer (`hex` in `/api/readDictateSettings`), as one status row: a short
//! status line (Hex isn't running, isn't installed, has no model ready, is running), one plain
//! line of help, and the buttons that fix it.
//!
//! - Hex 3 not found: Install Hex and a link to Hex's website. On this computer the app asks for
//!   the system presence prompt and runs Hex's own installer (`hex_install_plan.rs`,
//!   `app/voice_agent/hex_install.rs`) after a confirm dialog that shows the command; a remote
//!   computer's Hex is installed by its gxserver (`/api/managedTools`).
//! - Hex not running: Open Hex (this computer, macOS) which then checks again by itself, and
//!   Check again.
//! - Hex running: "Hex is running" with a picker of the models it has ready.
//!
//! CDXC:Dictate 2026-11-08 DECISION: User, of the old "Hex isn't running on this computer. Open
//! Hex." row: "omg this message is SO BAD. Please fix". The title no longer repeats the engine
//! name, "Open Hex" is a button instead of a sentence said twice, and each state is a status
//! line, one help line and its buttons. User: "also put link to their website please so ppl can
//! dl from there if they want: https://anoma.ly/hex/ - please make this section look nice!".
//! SEE-ALSO: server/src/voice_agent/speech/hex.rs (CDXC:Dictate 2026-11-07), server/src/managed_tools/hex.rs.

use super::super::super::fields::{
    SettingsDialogSpec, dialog_footer, settings_dialog, tooltip_text,
};
use super::hex_install_plan::{HEX_HOST, HEX_SITE, plan};
use super::*;
use gpui::{ClickEvent, FocusHandle, FontWeight};

/// The Hex model picker's width: room for the longest model names Hex lists ("SenseVoice Small").
const HEX_MODEL_PICKER_WIDTH: f32 = 240.0;

/// How often the row re-reads the install job while Hex installs.
const INSTALL_POLL: Duration = Duration::from_secs(2);

/// How often, and how many times, the row checks for Hex after Open Hex.
const OPEN_POLL: Duration = Duration::from_millis(1500);
const OPEN_POLLS: usize = 14;

/// Homebrew's website, for a Mac without it.
const BREW_SITE: &str = "https://brew.sh";

/// Where Install Hex on this computer stands (`hexInstallState` from the app).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HexInstallRun {
    /// `asking` (the system prompt), `installing`, `failed` or `noHomebrew`.
    pub(crate) phase: String,
    /// The installer's last lines when it failed.
    pub(crate) tail: String,
}

/// What the Hex row says for one state.
struct HexStatus {
    title: String,
    help: Option<String>,
    /// The numbered "what to do in Hex" lines under the help.
    steps: Vec<&'static str>,
}

/// The system prompt's name on this computer, for the "approve it" lines.
const fn presence_words() -> &'static str {
    if cfg!(target_os = "macos") {
        "Touch ID or your password"
    } else if cfg!(target_os = "windows") {
        "Windows Hello"
    } else {
        "your password"
    }
}

const SETUP_STEPS: [&str; 3] = [
    "Install Hex.",
    "Open Hex and allow the microphone.",
    "Download a model in Hex's settings: Parakeet v3 for English and 24 European languages, Whisper for the rest.",
];

impl VoiceAgentTab {
    /// Hex is offered where Hex 3 ships for the shown computer.
    pub(super) fn hex_offered(answer: &Value) -> bool {
        answer["hex"]["available"] == true
    }

    /// `hexInstallState` and `hexOpenFailed` from the app.
    pub(crate) fn receive_hex_payload(&mut self, kind: &str, cx: &mut Context<Self>) {
        let Some(message) = self.store.read(cx).host_payload(kind).cloned() else {
            return;
        };
        if kind == "hexOpenFailed" {
            self.dictate.hex_opening = None;
            self.dictate.action_error = message["message"].as_str().map(str::to_string);
            cx.notify();
            return;
        }
        let phase = message["phase"].as_str().unwrap_or_default();
        let tail = message["tail"].as_str().unwrap_or_default().to_string();
        match phase {
            "done" => {
                self.dictate.hex_install_run = None;
                self.refresh_dictate_quietly(cx);
            }
            "declined" => {
                self.dictate.hex_install_run = None;
                self.dictate.action_error =
                    Some("Hex wasn't installed: it wasn't approved.".to_string());
            }
            _ => {
                self.dictate.hex_install_run = Some(HexInstallRun {
                    phase: phase.to_string(),
                    tail,
                })
            }
        }
        cx.notify();
    }

    /// The Settings preview opens each install step without clicking (`dictation-install-*`).
    pub(crate) fn apply_hex_preview_state(&mut self, cx: &mut Context<Self>) {
        let preview = self.store.read(cx).request().preview_state.clone();
        let run = |phase: &str, tail: &str| {
            Some(HexInstallRun {
                phase: phase.to_string(),
                tail: tail.to_string(),
            })
        };
        match preview.as_deref() {
            Some("dictation-install-confirm") => self.dictate.hex_install_confirm = true,
            // As if Install Hex was confirmed: the stubbed app answers over time.
            Some("dictation-install-autorun") => {
                self.dictate.hex_install_run = run("asking", "");
                post_store_message(&self.store, json!({ "type": "hexInstallRun" }), cx);
            }
            Some("dictation-install-asking") => self.dictate.hex_install_run = run("asking", ""),
            Some("dictation-install-running") => {
                self.dictate.hex_install_run = run("installing", "")
            }
            Some("dictation-install-failed") => {
                self.dictate.hex_install_run = run(
                    "failed",
                    "==> Fetching downloads for: hex\nError: anomalyco/tap/hex: Download failed on Cask 'hex' with message: Failed to connect to github.com port 443\nError: Cask 'hex' definition is invalid",
                )
            }
            Some("dictation-install-nobrew") => {
                self.dictate.hex_install_run = run("noHomebrew", "")
            }
            _ => {}
        }
    }

    /// The status line and help for the Hex row's state.
    fn hex_status(&self, hex: &Value, state: &str, installing: bool) -> HexStatus {
        let here = if self.dictate.machine.is_none() {
            "this computer"
        } else {
            "that computer"
        };
        let none = |title: &str, help: Option<String>| HexStatus {
            title: title.to_string(),
            help,
            steps: Vec::new(),
        };
        if let Some(run) = &self.dictate.hex_install_run {
            return match run.phase.as_str() {
                "asking" => none(
                    "Waiting for your approval",
                    Some(format!(
                        "Confirm with {} to let Ghostex download and install Hex.",
                        presence_words()
                    )),
                ),
                "installing" => none(
                    "Installing Hex…",
                    Some(
                        "Ghostex is running Hex's installer. It takes a minute or two.".to_string(),
                    ),
                ),
                "noHomebrew" => none(
                    "Homebrew isn't installed",
                    Some(format!(
                        "Ghostex installs Hex on a Mac with Homebrew, which {here} doesn't have. Download Hex from its website instead, or install Homebrew first."
                    )),
                ),
                _ => none(
                    "Hex didn't install",
                    Some("Hex's installer stopped with this:".to_string()),
                ),
            };
        }
        if installing {
            return none(
                "Installing Hex…",
                Some("Ghostex finds Hex by itself when it is installed.".to_string()),
            );
        }
        let problem = hex["problem"]["message"].as_str().map(str::to_string);
        match state {
            "notInstalled" => {
                let older = hex["olderVersion"].as_str();
                HexStatus {
                    title: if older.is_some() {
                        "Hex 3 isn't installed".to_string()
                    } else {
                        "Hex isn't installed".to_string()
                    },
                    help: Some(match older {
                        Some(version) => format!(
                            "{} has Hex {version}, which Ghostex can't dictate with. Hex 3 is a free app that turns your speech into text on your computer.",
                            if self.dictate.machine.is_none() { "This computer" } else { "That computer" }
                        ),
                        None => "Dictation runs on Hex, a free app that turns your speech into text on your computer.".to_string(),
                    }),
                    steps: SETUP_STEPS.to_vec(),
                }
            }
            "notRunning" => none(
                "Hex isn't running",
                Some(if self.dictate.machine.is_none() {
                    "Dictation needs Hex open. Turn on Open at login in Hex so it starts with your computer.".to_string()
                } else {
                    "Dictation needs Hex open on that computer. Turn on Open at login in Hex there so it starts with that computer.".to_string()
                }),
            ),
            "ready" => {
                let no_models = hex["models"].as_array().is_none_or(Vec::is_empty);
                match (no_models, problem) {
                    (true, _) => none(
                        "Hex has no model ready",
                        Some(
                            "In Hex's settings, under Models, download a model. Ghostex lists it here once it is ready."
                                .to_string(),
                        ),
                    ),
                    (false, Some(message)) => none("Hex is running", Some(message)),
                    (false, None) => none("Hex is running", None),
                }
            }
            _ => none("Hex needs attention", problem),
        }
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
        let local = self.dictate.machine.is_none();
        let installing = self.dictate.hex_install.is_some();
        let run_phase = self
            .dictate
            .hex_install_run
            .as_ref()
            .map(|run| run.phase.clone());
        let status = self.hex_status(hex, state, installing);
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
        let hex_link = self.hex_link(
            p,
            HEX_SITE,
            &format!("{HEX_HOST}/hex"),
            "dictate-hex-site",
            cx,
        );
        let open_hex = {
            let opening = self.dictate.hex_opening.is_some();
            settings_button(
                p,
                "dictate-hex-open",
                if opening { "Opening…" } else { "Open Hex" },
                None,
                ButtonVariant::Default,
                opening,
                None,
                |page: &mut Self, _window, cx| page.open_hex(cx),
                cx,
            )
        };
        // Open Hex starts the app on this computer, where the app can (macOS).
        let can_open = local && cfg!(target_os = "macos");
        let mut compact = false;
        let controls: Vec<AnyElement> = match (run_phase.as_deref(), state) {
            (Some("asking" | "installing"), _) => vec![settings_button(
                p,
                "dictate-hex-install",
                if run_phase.as_deref() == Some("asking") {
                    "Waiting…"
                } else {
                    "Installing…"
                },
                None,
                ButtonVariant::Default,
                true,
                None,
                |_: &mut Self, _window, _cx| {},
                cx,
            )],
            (Some("noHomebrew"), _) => vec![
                self.hex_open_site_button(
                    p,
                    "Download Hex",
                    HEX_SITE,
                    ButtonVariant::Default,
                    "dictate-hex-download",
                    cx,
                ),
                check_again,
                self.hex_link(p, BREW_SITE, "brew.sh", "dictate-brew-site", cx),
            ],
            (Some("failed"), _) => vec![
                self.hex_install_button(p, "Try again", false, cx),
                check_again,
                hex_link,
            ],
            (_, "notInstalled") => vec![
                self.hex_install_button(
                    p,
                    if installing {
                        "Installing…"
                    } else if hex["olderVersion"].is_string() {
                        "Install Hex 3"
                    } else {
                        "Install Hex"
                    },
                    installing,
                    cx,
                ),
                hex_link,
            ],
            (_, "ready") => {
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
                    let mut buttons = Vec::new();
                    if can_open {
                        buttons.push(open_hex);
                    }
                    buttons.push(check_again);
                    buttons
                } else {
                    let value = hex["model"]
                        .as_str()
                        .or(answer["settings"]["hexModel"].as_str())
                        .unwrap_or_default()
                        .to_string();
                    compact = status.help.is_none();
                    vec![
                        settings_select(
                            self,
                            p,
                            "dictate-hex-model",
                            &options,
                            &value,
                            Some(HEX_MODEL_PICKER_WIDTH),
                            false,
                            None,
                            |page: &mut Self, value, _window, cx| {
                                page.set_dictate(json!({ "hexModel": value }), cx)
                            },
                            window,
                            cx,
                        ),
                        check_again,
                    ]
                }
            }
            (_, "notRunning") => {
                let mut buttons = Vec::new();
                if can_open {
                    buttons.push(open_hex);
                }
                buttons.push(check_again);
                buttons
            }
            _ => vec![check_again],
        };
        let plan = plan();
        let failed = run_phase.as_deref() == Some("failed");
        let tail = self
            .dictate
            .hex_install_run
            .as_ref()
            .filter(|_| failed)
            .map(|run| run.tail.clone());
        let version = hex["version"].as_str().unwrap_or_default();
        let tooltip = (state == "ready").then(|| {
            let place = if local {
                "this computer"
            } else {
                "that computer"
            };
            format!("Hex {version}. Recordings stay on {place}.")
        });
        let body = hex_body(
            p,
            &self.store,
            &status,
            tail.map(|tail| (tail, plan.display)),
            (run_phase.as_deref() == Some("installing")).then_some(plan.display),
            controls,
            compact,
        );
        let mut spec = RowSpec::new(status.title.clone());
        if let Some(tooltip) = tooltip {
            spec = spec.description(tooltip);
        }
        if !compact {
            spec = spec.wide();
        }
        Some(setting_row(p, "dictateHex", spec, None, body, cx))
    }

    /// A link-styled control that opens `url` in the browser.
    fn hex_link(
        &self,
        p: &SettingsPalette,
        url: &'static str,
        label: &str,
        id: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(id)
            .flex_shrink_0()
            .cursor_pointer()
            .text_size(px(13.0))
            .line_height(px(18.0))
            .text_color(hsla(p.primary))
            .hover(|this| this.underline())
            .tooltip(tooltip_text(url))
            .on_click(cx.listener(move |page, _: &ClickEvent, _window, cx| {
                post_store_message(
                    &page.store,
                    json!({ "type": "openExternalUrl", "url": url }),
                    cx,
                );
            }))
            .child(format!("{label} ↗"))
            .into_any_element()
    }

    fn hex_open_site_button(
        &self,
        p: &SettingsPalette,
        label: &'static str,
        url: &'static str,
        variant: ButtonVariant,
        id: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_button(
            p,
            id,
            label,
            None,
            variant,
            false,
            None,
            move |page: &mut Self, _window, cx| {
                post_store_message(
                    &page.store,
                    json!({ "type": "openExternalUrl", "url": url }),
                    cx,
                );
            },
            cx,
        )
    }

    /// Install Hex: on this computer, the confirm dialog first; on a remote one, its gxserver.
    fn hex_install_button(
        &self,
        p: &SettingsPalette,
        label: &'static str,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_button(
            p,
            "dictate-hex-install",
            label,
            None,
            ButtonVariant::Default,
            busy,
            None,
            |page: &mut Self, window, cx| {
                if page.dictate.machine.is_none() {
                    page.dictate.hex_install_confirm = true;
                    page.dictate.hex_confirm_focus.focus(window, cx);
                    cx.notify();
                } else {
                    page.install_hex(cx);
                }
            },
            cx,
        )
    }

    /// Open Hex, then check again by itself until it runs (or about 20 seconds pass).
    fn open_hex(&mut self, cx: &mut Context<Self>) {
        if self.dictate.hex_opening.is_some() {
            return;
        }
        self.dictate.action_error = None;
        post_store_message(&self.store, json!({ "type": "hexOpen" }), cx);
        self.dictate.hex_opening = Some(cx.spawn(async move |page, cx| {
            for _ in 0..OPEN_POLLS {
                cx.background_executor().timer(OPEN_POLL).await;
                let Ok(running) = page.update(cx, |page, cx| {
                    let running = page
                        .dictate
                        .answer
                        .as_ref()
                        .is_some_and(|answer| answer["hex"]["state"] != "notRunning");
                    if !running {
                        page.refresh_dictate_quietly(cx);
                    }
                    running
                }) else {
                    return;
                };
                if running {
                    break;
                }
            }
            let _ = page.update(cx, |page, cx| {
                page.dictate.hex_opening = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// The Install Hex confirm: what happens, the exact command, and then the system prompt.
    pub(crate) fn render_hex_install_dialog(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.dictate.hex_install_confirm {
            return None;
        }
        let p = &p.nested_dialog();
        let command = plan().display;
        let cancel = settings_button(
            p,
            "dictate-hex-install-cancel",
            "Cancel",
            None,
            ButtonVariant::Outline,
            false,
            None,
            |page: &mut Self, _window, cx| {
                page.dictate.hex_install_confirm = false;
                cx.notify();
            },
            cx,
        );
        let confirm = settings_button(
            p,
            "dictate-hex-install-confirm",
            "Install Hex",
            None,
            ButtonVariant::Primary,
            false,
            None,
            |page: &mut Self, _window, cx| {
                page.dictate.hex_install_confirm = false;
                page.dictate.action_error = None;
                page.dictate.hex_install_run = Some(HexInstallRun {
                    phase: "asking".to_string(),
                    tail: String::new(),
                });
                post_store_message(&page.store, json!({ "type": "hexInstallRun" }), cx);
                cx.notify();
            },
            cx,
        );
        let focus: FocusHandle = self.dictate.hex_confirm_focus.clone();
        let body = vec![
            div()
                .text_size(px(13.0))
                .line_height(px(18.0))
                .text_color(hsla(p.muted))
                .child("This is the command Ghostex will run:")
                .into_any_element(),
            command_block(p, command),
            div()
                .text_size(px(13.0))
                .line_height(px(18.0))
                .text_color(hsla(p.muted))
                .child(format!(
                    "Next, your computer asks for {} so only you can allow it.",
                    presence_words()
                ))
                .into_any_element(),
        ];
        Some(settings_dialog(
            p,
            SettingsDialogSpec::new("dictate-hex-install-dialog", "Install Hex?")
                .width(480.0)
                .spacing(20.0, 14.0)
                .description(format!(
                    "Ghostex will download and install Hex from {HEX_HOST}."
                )),
            body,
            Some(dialog_footer(vec![cancel, confirm])),
            Some(&focus),
            |page: &mut Self, _window, cx| {
                page.dictate.hex_install_confirm = false;
                cx.notify();
            },
            window,
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

/// The row's body: the help line, the numbered steps, the installer's output and command when it
/// failed (or the command while it runs), then the buttons. A `compact` row (Hex running with
/// nothing to say) is just its buttons, on the right of the status line.
fn hex_body(
    p: &SettingsPalette,
    store: &gpui::Entity<super::super::super::store::SettingsStore>,
    status: &HexStatus,
    failure: Option<(String, &'static str)>,
    running_command: Option<&'static str>,
    controls: Vec<AnyElement>,
    compact: bool,
) -> AnyElement {
    let controls_row = h_flex()
        .gap(px(8.0))
        .items_center()
        .flex_wrap()
        .children(controls)
        .into_any_element();
    if compact {
        return controls_row;
    }
    let muted_text = |text: String| {
        div()
            .text_size(px(13.0))
            .line_height(px(18.5))
            .text_color(hsla(p.muted))
            .child(text)
    };
    v_flex()
        .w_full()
        .gap(px(10.0))
        // A failed install leads with its cause; its own help line only introduced the output.
        .children(
            status
                .help
                .clone()
                .filter(|_| failure.is_none())
                .map(&muted_text),
        )
        .when(!status.steps.is_empty(), |this| {
            this.child(
                v_flex().w_full().gap(px(4.0)).children(
                    status
                        .steps
                        .iter()
                        .enumerate()
                        .map(|(index, step)| numbered_step(p, index + 1, step)),
                ),
            )
        })
        .when_some(running_command, |this, command| {
            this.child(command_block(p, command))
        })
        // CDXC:Dictation 2026-11-08 WHY: the installer's raw output (Homebrew's "==> Fetching…",
        // "Error: …") was the row's main content. The cause comes first in a sentence; the output
        // is under Details with Copy (User, Q2021 on the remote tile audit page).
        .when_some(failure, |this, (tail, command)| {
            this.child(muted_text(install_failure_cause(&tail).to_string()))
                .child(super::super::super::fields::notice::raw_details(
                    p,
                    "dictate-hex-install-output",
                    tail,
                    store,
                ))
                .child(muted_text("To install it yourself, run this:".to_string()))
                .child(command_block(p, command))
        })
        .child(controls_row)
        .into_any_element()
}

/// Why Hex's installer stopped, in a sentence, from the lines it printed.
fn install_failure_cause(output: &str) -> &'static str {
    let output = output.to_ascii_lowercase();
    if [
        "failed to connect",
        "could not resolve host",
        "connection timed out",
        "port 443",
    ]
    .iter()
    .any(|sign| output.contains(sign))
    {
        "Hex's download couldn't reach github.com. Check this computer's internet connection, then try again."
    } else if output.contains("permission denied") {
        "Hex's installer wasn't allowed to write its files."
    } else {
        "Hex's installer stopped before Hex was installed."
    }
}

/// One "1. Install Hex." line: the number in a muted column, the words in the body colour.
fn numbered_step(p: &SettingsPalette, number: usize, text: &str) -> AnyElement {
    h_flex()
        .w_full()
        .items_start()
        .gap(px(10.0))
        .text_size(px(13.0))
        .line_height(px(18.5))
        .child(
            div()
                .flex_shrink_0()
                .w(px(14.0))
                .text_color(hsla(p.muted))
                .child(format!("{number}.")),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_color(hsla(p.foreground_alpha(0.82)))
                .child(text.to_string()),
        )
        .into_any_element()
}

/// A command in a monospace box that wraps, so the whole line can be read and copied by hand.
fn command_block(p: &SettingsPalette, command: &str) -> AnyElement {
    mono_block(p, command.to_string(), p.foreground_alpha(0.9))
}

fn mono_block(p: &SettingsPalette, text: String, color: gpui::Rgba) -> AnyElement {
    div()
        .w_full()
        .px(px(10.0))
        .py(px(8.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(hsla(p.hairline))
        .bg(hsla(p.foreground_alpha(0.035)))
        .font_family(MODAL_MONO_FONT)
        .text_size(px(12.0))
        .line_height(px(17.0))
        .font_weight(FontWeight::NORMAL)
        .text_color(hsla(color))
        .whitespace_normal()
        .child(text)
        .into_any_element()
}
