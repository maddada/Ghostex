//! Collapsible CLIProxyAPI card on Settings > Accounts (`render_cliproxy_card`) and the in-app
//! CLIProxyAPI setup instructions sheet (`render_cliproxy_guide`).
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    SizedButtonSize, SizedButtonVariant, settings_dialog_sheet, settings_icon,
    settings_sized_button,
};
use super::super::super::model::SettingsTabId;
use super::super::super::palette::SettingsPalette;
use super::super::super::store::{store_host_message, store_machine_gxserver_rpc};
use super::widgets::{TagTone, status_tag};
use super::{AccountsTab, Pane};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::json;
use std::time::Duration;

const CLIPROXY_RELEASES_URL: &str = "https://github.com/router-for-me/CLIProxyAPI/releases";

const CLIPROXY_CONFIG_SAMPLE: &str = r#"config-version: 8

server:
  host: "0.0.0.0"
  port: 8317

access:
  api-keys:
    - "replace-with-a-random-client-key"

management:
  allow-remote: true
  secret-key: "replace-with-a-random-management-key"

routing:
  strategy: "fill-first"
  session-affinity: true"#;

impl AccountsTab {
    pub(crate) fn open_cliproxy_guide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cliproxy_guide_open = true;
        self.cliproxy_guide_needs_focus = false;
        self.cliproxy_guide_focus.focus(window, cx);
        cx.notify();
    }

    /// Turns on the built-in CLIProxyAPI extension on this computer or on the picked remote
    /// machine, then refreshes Accounts and opens Add a hub.
    pub(crate) fn enable_cliproxy_extension(&mut self, cx: &mut Context<Self>) {
        if self.cliproxy_enabling {
            return;
        }
        let machine = self.client.read(cx).machine().map(str::to_string);
        let this = cx.weak_entity();
        if let Some(machine_id) = machine {
            self.cliproxy_enabling = true;
            cx.notify();
            let store = self.store.clone();
            store_machine_gxserver_rpc(
                &store,
                Some(&machine_id),
                "/api/machineExtensions",
                json!({ "action": "set", "id": "cliProxyApi", "on": true }),
                Duration::from_secs(30),
                move |result, cx| {
                    let _ = this.update(cx, |page, cx| {
                        page.cliproxy_enabling = false;
                        match result {
                            Ok(_) => {
                                let after = cx.weak_entity();
                                page.account_request(
                                    json!({ "operation": "list", "refresh": true }),
                                    Some(Box::new(move |ok, cx| {
                                        if ok {
                                            let _ = after.update(cx, |page, cx| {
                                                page.cliproxy_guide_open = false;
                                                page.select_pane(Pane::AddHub, cx);
                                            });
                                        }
                                    })),
                                    cx,
                                );
                            }
                            Err(error) => {
                                page.store.update(cx, |store, cx| {
                                    store.toast(
                                        "warning",
                                        "CLIProxyAPI was not enabled",
                                        &error,
                                        cx,
                                    )
                                });
                                cx.notify();
                            }
                        }
                    });
                },
                cx,
            );
            return;
        }
        self.cliproxy_enabling = true;
        self.store.update(cx, |store, cx| {
            store.update_setting("cliProxyApiEnabled", json!(true), cx);
        });
        cx.notify();
        // Wait briefly for the settings file save before refreshing gxserver's accounts list.
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(350))
                .await;
            let _ = this.update(cx, |page, cx| {
                page.cliproxy_enabling = false;
                let after = cx.weak_entity();
                page.account_request(
                    json!({ "operation": "list", "refresh": true }),
                    Some(Box::new(move |ok, cx| {
                        if ok {
                            let _ = after.update(cx, |page, cx| {
                                page.cliproxy_guide_open = false;
                                if page
                                    .client
                                    .read(cx)
                                    .data
                                    .as_ref()
                                    .and_then(|d| d.hubs())
                                    .is_some_and(|hubs| hubs.is_empty())
                                {
                                    page.select_pane(Pane::AddHub, cx);
                                }
                            });
                        }
                    })),
                    cx,
                );
            });
        })
        .detach();
    }

    /// Collapsible card in Settings > Accounts explaining when CLIProxyAPI is better than
    /// built-in account management, with one-click enable, setup instructions, and machine
    /// switching / hub connection controls.
    pub(crate) fn render_cliproxy_card(
        &mut self,
        p: &SettingsPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let data = self.client.read(cx).data.clone();
        let hubs = data.as_ref().and_then(|d| d.hubs());
        let is_local = self.client.read(cx).machine().is_none();
        let extension_on =
            hubs.is_some() || (is_local && self.store.read(cx).bool("cliProxyApiEnabled"));
        let hub_count = hubs.as_ref().map_or(0, Vec::len);
        let first_hub_id = hubs.as_ref().and_then(|list| list.first()).map(|h| h.id());
        let collapsed = self.cliproxy_card_collapsed;
        let enabling = self.cliproxy_enabling;
        let busy = self.client.read(cx).busy || enabling;

        let badge = if extension_on {
            if hub_count > 0 {
                status_tag(
                    p,
                    if hub_count == 1 {
                        "1 hub connected".to_string()
                    } else {
                        format!("{hub_count} hubs connected")
                    },
                    TagTone::Plain,
                )
            } else {
                status_tag(p, "Enabled", TagTone::Plain)
            }
        } else {
            status_tag(p, "Optional", TagTone::Off)
        };

        let chevron = if collapsed {
            "modals/settings/chevron-right.svg"
        } else {
            "modals/settings/chevron-down.svg"
        };

        let header = h_flex()
            .id("accounts-cliproxy-card-toggle")
            .w_full()
            .px(px(14.0))
            .py(px(11.0))
            .gap(px(10.0))
            .items_center()
            .cursor_pointer()
            .hover(|this| this.bg(hsla(p.foreground_alpha(0.03))))
            .on_click(cx.listener(|page, _: &ClickEvent, _window, cx| {
                page.cliproxy_card_collapsed = !page.cliproxy_card_collapsed;
                cx.notify();
            }))
            .child(settings_icon("modals/settings/server.svg", 16.0, p.foreground))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(p.foreground))
                            .child("CLIProxyAPI for multiple machines"),
                    )
                    .when(collapsed, |this| {
                        this.child(
                            div()
                                .text_size(px(12.0))
                                .line_height(px(16.0))
                                .text_color(hsla(p.muted))
                                .child("One always-on machine holds your Claude and Codex logins so other machines stop re-logging in."),
                        )
                    }),
            )
            .child(badge)
            .child(settings_icon(chevron, 14.0, p.muted));

        let mut card = v_flex()
            .id("accounts-cliproxy-card")
            .w_full()
            .rounded(px(MODAL_RADIUS_SECTION))
            .border_1()
            .border_color(hsla(p.hairline))
            .bg(hsla(p.raised))
            .overflow_hidden()
            .child(header);

        if collapsed {
            return card.into_any_element();
        }

        // Connected remote machines for quick switching to the CLIProxyAPI host.
        let picked_machine = self.store.read(cx).picked_machine().map(str::to_string);
        let remote_choices: Vec<(Option<String>, String)> = {
            let store = self.store.read(cx);
            super::super::machine_picker::machine_choices(
                &store.value("remoteMachines"),
                store.host_payload("remoteMachineConnectStates"),
            )
            .into_iter()
            .filter(|choice| choice.unavailable.is_none())
            .filter(|choice| choice.machine_id != picked_machine)
            .map(|choice| (choice.machine_id, choice.name))
            .collect()
        };

        let mut actions: Vec<AnyElement> = Vec::new();
        if !extension_on {
            actions.push(settings_sized_button(
                p,
                "accounts-cliproxy-enable",
                if enabling {
                    "Enabling CLIProxyAPI…"
                } else {
                    "Enable CLIProxyAPI"
                },
                Some("modals/settings/server.svg"),
                None,
                SizedButtonVariant::Primary,
                SizedButtonSize::Sm,
                busy,
                None,
                |page: &mut Self, _window, cx| page.enable_cliproxy_extension(cx),
                cx,
            ));
        } else if let Some(hub_id) = first_hub_id {
            let open_id = hub_id.clone();
            actions.push(settings_sized_button(
                p,
                "accounts-cliproxy-open-hub",
                "Manage hub",
                Some("modals/settings/server.svg"),
                None,
                SizedButtonVariant::Primary,
                SizedButtonSize::Sm,
                false,
                None,
                move |page: &mut Self, _window, cx| {
                    page.select_pane(Pane::Hub(open_id.clone()), cx)
                },
                cx,
            ));
            actions.push(settings_sized_button(
                p,
                "accounts-cliproxy-add-another-hub",
                "Point at another hub",
                Some("modals/settings/plus.svg"),
                None,
                SizedButtonVariant::Outline,
                SizedButtonSize::Sm,
                false,
                None,
                |page: &mut Self, _window, cx| page.select_pane(Pane::AddHub, cx),
                cx,
            ));
        } else {
            actions.push(settings_sized_button(
                p,
                "accounts-cliproxy-add-hub",
                "Point at a CLIProxyAPI hub",
                Some("modals/settings/plus.svg"),
                None,
                SizedButtonVariant::Primary,
                SizedButtonSize::Sm,
                false,
                None,
                |page: &mut Self, _window, cx| page.select_pane(Pane::AddHub, cx),
                cx,
            ));
        }

        actions.push(settings_sized_button(
            p,
            "accounts-cliproxy-instructions",
            "Setup instructions",
            Some("modals/settings/info-circle.svg"),
            None,
            SizedButtonVariant::Outline,
            SizedButtonSize::Sm,
            false,
            None,
            |page: &mut Self, window, cx| page.open_cliproxy_guide(window, cx),
            cx,
        ));

        if !remote_choices.is_empty() {
            for (index, (machine_id, name)) in remote_choices.into_iter().take(3).enumerate() {
                let target_id = machine_id.clone();
                let label = format!("Switch to {name}");
                actions.push(settings_sized_button(
                    p,
                    SharedString::from(format!("accounts-cliproxy-switch-machine-{index}")),
                    label,
                    None,
                    None,
                    SizedButtonVariant::Ghost,
                    SizedButtonSize::Sm,
                    false,
                    Some("Switch Settings > Accounts to the machine running CLIProxyAPI.".into()),
                    move |page: &mut Self, _window, cx| {
                        let store = page.store.clone();
                        let next = target_id.clone();
                        store.update(cx, |store, cx| store.set_picked_machine(next, cx));
                    },
                    cx,
                ));
            }
        } else {
            actions.push(settings_sized_button(
                p,
                "accounts-cliproxy-remote-machines",
                "Connect a remote machine",
                None,
                None,
                SizedButtonVariant::Ghost,
                SizedButtonSize::Sm,
                false,
                None,
                |page: &mut Self, _window, cx| {
                    let store = page.store.clone();
                    store.update(cx, |store, cx| {
                        store.set_active_tab(SettingsTabId::Remote, cx)
                    });
                },
                cx,
            ));
        }

        card = card.child(
            v_flex()
                .w_full()
                .px(px(14.0))
                .pb(px(14.0))
                .pt(px(10.0))
                .gap(px(10.0))
                .border_t_1()
                .border_color(hsla(p.hairline))
                .child(
                    div()
                        .text_size(px(12.5))
                        .line_height(px(18.5))
                        .text_color(hsla(p.muted))
                        .child("Built-in account management in Ghostex works well when you have 1 to 2 accounts across 1 to 2 machines, since re-logging in rarely gets in the way. Above that, CLIProxyAPI is a better fit."),
                )
                .child(
                    div()
                        .text_size(px(12.5))
                        .line_height(px(18.5))
                        .text_color(hsla(p.muted))
                        .child("Run CLIProxyAPI on one always-on machine (it acts as your account server) and point every Ghostex machine at it. You sign in to Claude and Codex once there instead of re-logging in on every computer."),
                )
                .child(h_flex().w_full().flex_wrap().gap(px(8.0)).children(actions)),
        );

        card.into_any_element()
    }

    /// Full step-by-step instructions sheet for setting up CLIProxyAPI on an always-on machine
    /// and connecting Ghostex to it.
    pub(crate) fn render_cliproxy_guide(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.cliproxy_guide_open {
            return None;
        }
        if self.cliproxy_guide_needs_focus {
            self.cliproxy_guide_needs_focus = false;
            self.cliproxy_guide_focus.focus(window, cx);
        }
        let p = &p.nested_dialog();
        let data = self.client.read(cx).data.clone();
        let is_local = self.client.read(cx).machine().is_none();
        let extension_on = data.as_ref().and_then(|d| d.hubs()).is_some()
            || (is_local && self.store.read(cx).bool("cliProxyApiEnabled"));
        let enabling = self.cliproxy_enabling;

        let heading = |text: &str| {
            div()
                .text_size(px(13.0))
                .line_height(px(18.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(hsla(p.foreground))
                .child(text.to_string())
                .into_any_element()
        };
        let paragraph = |text: &str| {
            div()
                .text_size(px(12.5))
                .line_height(px(18.5))
                .text_color(hsla(p.muted))
                .child(text.to_string())
                .into_any_element()
        };
        let step = |children: Vec<AnyElement>| {
            v_flex()
                .w_full()
                .gap(px(8.0))
                .children(children)
                .into_any_element()
        };

        let warning_box = v_flex()
            .w_full()
            .gap(px(6.0))
            .p(px(12.0))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(modal_rgba(0xfbbf24, 0.35)))
            .bg(hsla(modal_rgba(0xfbbf24, 0.08)))
            .child(
                h_flex()
                    .gap(px(8.0))
                    .items_center()
                    .child(settings_icon(
                        "modals/settings/alert-triangle.svg",
                        15.0,
                        modal_rgba(0xfbbf24, 0.95),
                    ))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(hsla(p.foreground))
                            .child("Important warnings before you start"),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(18.0))
                    .text_color(hsla(p.foreground))
                    .child("1. Do not add Antigravity to CLIProxyAPI. Antigravity is known for banning accounts used through third-party proxies. Ghostex supports Claude and Codex hubs only, and won't start sessions on a hub that holds an Antigravity, Gemini, Vertex or AI Studio login."),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(18.0))
                    .text_color(hsla(p.foreground))
                    .child("2. Using 2 accounts of the same provider on the same PC is not recommended. Providers may flag or restrict subscription accounts when multiple logins of the same provider share one machine or proxy."),
            )
            .into_any_element();

        let releases_btn = settings_sized_button(
            p,
            "cliproxy-guide-releases",
            "Open CLIProxyAPI releases",
            Some("modals/settings/external-link.svg"),
            None,
            SizedButtonVariant::Outline,
            SizedButtonSize::Sm,
            false,
            None,
            |page: &mut Self, _window, cx| {
                store_host_message(
                    &page.store,
                    json!({ "type": "openExternalUrl", "url": CLIPROXY_RELEASES_URL }),
                    cx,
                );
            },
            cx,
        );

        let install_cmd = self.copy_command_block(
            p,
            "cliproxy-guide-brew".into(),
            "brew install cliproxyapi",
            cx,
        );
        let config_block = self.copy_command_block(
            p,
            "cliproxy-guide-config".into(),
            CLIPROXY_CONFIG_SAMPLE,
            cx,
        );
        let run_cmd = self.copy_command_block(
            p,
            "cliproxy-guide-run".into(),
            "cliproxyapi --config ~/.cli-proxy-api/config.yaml",
            cx,
        );

        let body = vec![
            warning_box,
            step(vec![
                heading("When CLIProxyAPI helps most"),
                paragraph(
                    "CLIProxyAPI works best when you use several machines and one of them stays on (for example a Mac mini, home server, or always-on desktop). That machine holds your Claude and Codex logins, so your laptops and other desktops stop asking you to re-log in.",
                ),
                paragraph(
                    "If you only have 1 to 2 accounts on 1 to 2 machines, Ghostex's built-in account management is simpler and needs no extra server.",
                ),
            ]),
            step(vec![
                heading("1. Install CLIProxyAPI on your always-on machine"),
                paragraph(
                    "On macOS, install with Homebrew, or download a prebuilt release binary for macOS, Linux, or Windows from GitHub.",
                ),
                install_cmd,
                h_flex().child(releases_btn).into_any_element(),
            ]),
            step(vec![
                heading("2. Create ~/.cli-proxy-api/config.yaml"),
                paragraph(
                    "Pick a strong client key (used by Claude and Codex sessions) and management key (used by Ghostex to list and sign in accounts). Keep routing.session-affinity set to true so each conversation stays on one account and keeps prompt caching warm. Use 127.0.0.1 instead of 0.0.0.0 if only that machine connects; otherwise keep the port on your Tailscale or private LAN.",
                ),
                config_block,
            ]),
            step(vec![
                heading("3. Start CLIProxyAPI"),
                paragraph(
                    "Run CLIProxyAPI on the always-on machine (or run it as a background service with launchd, systemd, or brew services start cliproxyapi so it stays up across restarts).",
                ),
                run_cmd,
            ]),
            step(vec![
                heading("4. Connect Ghostex on each machine"),
                paragraph(
                    "Enable the CLIProxyAPI extension in Ghostex, then click Add a hub in Settings > Accounts and enter your hub address (for example http://my-server.ts.net:8317 or http://127.0.0.1:8317), your client key, and your management key. Ghostex enables session affinity automatically when you connect with a management key.",
                ),
            ]),
            step(vec![
                heading("5. Sign in Claude and Codex accounts and route new sessions"),
                paragraph(
                    "Open the hub in Settings > Accounts and click Add Claude account or Add Codex account to sign in through your browser. Then open Claude rules or Codex rules and set New sessions start on to On the hub. Hub sessions automatically share your ~/.claude/CLAUDE.md, skills/, projects/, and history.jsonl.",
                ),
            ]),
        ];

        let primary_cta = if !extension_on {
            settings_sized_button(
                p,
                "cliproxy-guide-enable",
                if enabling {
                    "Enabling CLIProxyAPI…"
                } else {
                    "Enable CLIProxyAPI"
                },
                Some("modals/settings/server.svg"),
                None,
                SizedButtonVariant::Primary,
                SizedButtonSize::Sm,
                enabling,
                None,
                |page: &mut Self, _window, cx| page.enable_cliproxy_extension(cx),
                cx,
            )
        } else {
            settings_sized_button(
                p,
                "cliproxy-guide-add-hub",
                "Add a hub",
                Some("modals/settings/plus.svg"),
                None,
                SizedButtonVariant::Primary,
                SizedButtonSize::Sm,
                false,
                None,
                |page: &mut Self, _window, cx| {
                    page.cliproxy_guide_open = false;
                    page.select_pane(Pane::AddHub, cx);
                },
                cx,
            )
        };

        let close_btn = settings_sized_button(
            p,
            "cliproxy-guide-close",
            "Close",
            None,
            None,
            SizedButtonVariant::Outline,
            SizedButtonSize::Sm,
            false,
            None,
            |page: &mut Self, _window, cx| {
                page.cliproxy_guide_open = false;
                cx.notify();
            },
            cx,
        );

        let footer = h_flex()
            .w_full()
            .justify_end()
            .gap(px(8.0))
            .child(close_btn)
            .child(primary_cta)
            .into_any_element();

        let max_height = f32::from(window.viewport_size().height) * 0.9;
        let description = div()
            .text_size(px(12.0))
            .line_height(px(18.5))
            .child("Set up CLIProxyAPI on one always-on machine and connect your Ghostex machines to it.")
            .into_any_element();
        let focus = self.cliproxy_guide_focus.clone();

        Some(settings_dialog_sheet(
            p,
            "cliproxy-setup-guide",
            &focus,
            "Set up CLIProxyAPI",
            Some(description),
            700.0,
            Some(max_height),
            24.0,
            18.0,
            true,
            body,
            Some(footer),
            |page: &mut Self, _window, cx| {
                page.cliproxy_guide_open = false;
                cx.notify();
            },
            window,
            cx,
        ))
    }
}
