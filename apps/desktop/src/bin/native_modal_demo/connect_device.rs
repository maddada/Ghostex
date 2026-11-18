//! Preview of the Connect a phone window. States: `code` (default: the QR, waiting for a phone),
//! `preparing` (turning Easy Connect on, never answers), `failed` (turning on failed), `ask` (a phone
//! scanned: its three words and Approve), `connected` (after Approve), `computer` (Connect a
//! computer's code), `presence` (`ask` with Approve asking for Touch ID first), `helper` (the
//! Easy Connect helper isn't installed: Install runs a pretend install, then the code shows),
//! `helper-failed` (its install failed), `helper-installing` (an install that already runs),
//! `large` (the phone's code filling the window, as after a click on it), `tailscale` (the
//! Tailscale way with its checklist), `tailscale-off` (the way while Tailscale is switched off).
//! Connect a computer: `computer` (its code and the box for the other computer's code),
//! `computer-words` (a pasted code, waiting for Approve there), `computer-done` (connected, with
//! Connect both ways), `computer-password` (the other computer kept no key: its password),
//! `computer-both` (Connect both ways finished).
//! Asking for the code refused (this app's owner key waits for an Approve): `refused` (Connect a
//! device), `refused-computer` (Connect a computer's Code tab), `refused-headless` (its Headless
//! server tab).
//! SSH access off here: `ssh-off` (the phone's code asks to turn it on), `computer-ssh-off` (Connect
//! a computer: the code's square asks, the paste box works), `computer-done-ssh-off` (Connect both
//! ways offers Turn on SSH first); Turn on SSH pretends to succeed, `ssh-fail` in the state makes
//! it fail.
//! Linux without an OpenSSH server: `computer-ssh-missing` (its apt command with Copy, Install and
//! Turn on SSH; Install pretends to install, then succeeds), `computer-ssh-missing-fail` (Install
//! clicked, then fails), `computer-ssh-missing-nopm` (no known package manager: the short list, no Install),
//! `computer-ssh-installing` (Install running), `computer-ssh-stopped` (installed but not
//! running), `computer-done-ssh-missing` (Connect both ways offers Install SSH).
use super::connect_device_modal::*;
use super::settings_modal::tabs::remote::tailscale_checklist_of_status;
use gpui::{App, AppContext as _, Entity, WindowHandle};
use gpui_component::Root;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

type Slot = Rc<RefCell<Option<(WindowHandle<Root>, Entity<GpuiConnectDeviceWindow>)>>>;

/// A code as long as a real one (with its secret), so the QR is as dense as the real code.
const CODE: &str = "ghostex-ec1:eyJ2IjoxLCJhZGRyZXNzIjoidGNDanNxLTNBQjBoTWFnYm1NOWg3QkxEQW5qNXdFNHI3Si1HZlE3QlkwVGstWGtnVlB4dXJsb3NBMFExTVg1eDFMTUQiLCJuYW1lIjoiTW9oYW1hZCdzIExhcHRvcCIsInVzZXIiOiJtYWRkYSIsInBvcnQiOjU4NzQ0LCJzc2hQb3J0IjoyMiwic2VjcmV0IjoiNnBSdk5xS2VKd0x4QkgzX3l0VjlzQSJ9";

fn with_view(
    slot: &Slot,
    cx: &mut gpui::AsyncApp,
    run: impl FnOnce(&mut GpuiConnectDeviceWindow, &mut gpui::Context<GpuiConnectDeviceWindow>),
) {
    let target = slot.borrow().clone();
    if let Some((window, view)) = target {
        let _ = cx.update(|cx| {
            let _ = window.update(cx, |_root, _window, cx| view.update(cx, run));
        });
    }
}

fn with_view_window(
    slot: &Slot,
    cx: &mut gpui::AsyncApp,
    run: impl FnOnce(
        &mut GpuiConnectDeviceWindow,
        &mut gpui::Window,
        &mut gpui::Context<GpuiConnectDeviceWindow>,
    ),
) {
    let target = slot.borrow().clone();
    if let Some((window, view)) = target {
        let _ = cx.update(|cx| {
            let _ = window.update(cx, |_root, window, cx| {
                view.update(cx, |modal, cx| run(modal, window, cx))
            });
        });
    }
}

/// The other computer's code the `computer-*` states paste.
const OTHER_CODE: &str = "ghostex-ec1:eyJ2IjoxLCJhZGRyZXNzIjoidGNEYW1CMGhNYWdibU05aDdCTERBbmo1d0U0cjdKLUdmUTdCWTBUay1Ya2dWUHh1cmxvc0EwUTFNWDV4MUxNRCIsIm5hbWUiOiJNYWNCb29rIFBybyIsInVzZXIiOiJtYWRkYSIsInBvcnQiOjU4NzQ0LCJzc2hQb3J0IjoyMiwic2VjcmV0IjoiUHZOcUtlSndMeEJIM195dFY5c0E2cFIifQ";

/// What this app answers while its owner key waits for an Approve (`owner_refusal`).
const REFUSAL: &str = "This computer already has an owner. On your phone, approve \"Let Mohamad's Laptop (Ghostex app) show the pairing code of …\" once, only if it shows the key words candle cobalt river pony; then Ghostex here can show the code and approve devices.";

/// What gxserver says about the SSH server in the Linux states.
fn ssh_server_of(state: &str) -> SshServer {
    if state.contains("ssh-stopped") {
        return SshServer {
            installed: Some(true),
            install_command: None,
        };
    }
    if state.contains("ssh-missing") || state.contains("ssh-installing") {
        return SshServer {
            installed: Some(false),
            install_command: (!state.contains("nopm"))
                .then(|| "sudo apt install openssh-server".to_string()),
        };
    }
    SshServer::default()
}

fn poll_answer(state: &str) -> ConnectPoll {
    if state.starts_with("refused") {
        return ConnectPoll {
            computer_name: Some("Mohamad's Laptop".to_string()),
            error: Some(REFUSAL.to_string()),
            ..ConnectPoll::default()
        };
    }
    let waiting = if matches!(state, "ask" | "presence" | "connected") {
        vec![WaitingDevice {
            name: "Mo's iPhone".to_string(),
            kind_label: "iPhone".to_string(),
            phone: true,
            words: vec!["harbor".into(), "koala".into(), "angle".into()],
            approval_id: "approval-1".to_string(),
        }]
    } else {
        Vec::new()
    };
    ConnectPoll {
        code: Some(ConnectCode {
            payload: format!("https://ghostex.dev/pair#{CODE}"),
            text: CODE.to_string(),
        }),
        computer_name: Some("Mohamad's Laptop".to_string()),
        waiting,
        paired_ids: vec!["pixel".to_string()],
        error: None,
    }
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let slot: Slot = Rc::new(RefCell::new(None));
    let host_slot = slot.clone();
    let state = demo.state.clone();
    let host: ConnectDeviceHost = Rc::new(move |command, cx: &mut App| {
        let slot = host_slot.clone();
        let state = state.clone();
        match command {
            ConnectDeviceCommand::Prepare => {
                cx.spawn(async move |cx| {
                    cx.background_executor().timer(Duration::from_millis(50)).await;
                    match state.as_str() {
                        "preparing" => with_view(&slot, cx, |modal, cx| {
                            modal.prepare_progress(
                                "Turning on Easy Connect…".to_string(),
                                cx,
                            )
                        }),
                        "helper" => with_view(&slot, cx, |modal, cx| modal.needs_helper(None, cx)),
                        "helper-failed" => with_view(&slot, cx, |modal, cx| {
                            modal.needs_helper(
                                Some("Ghostex couldn't download what the Easy Connect helper needs. Check that this computer is online, then try again.".to_string()),
                                cx,
                            )
                        }),
                        "helper-installing" => with_view(&slot, cx, |modal, cx| {
                            modal.helper_install_progress(
                                Some("Downloading the build tools… 42% of 66 MB".to_string()),
                                cx,
                            )
                        }),
                        "failed" => with_view(&slot, cx, |modal, cx| {
                            modal.prepared(
                                Err("SSH access could not be turned on. Remote Login is managed by your organization.".to_string()),
                                cx,
                            )
                        }),
                        _ => with_view(&slot, cx, |modal, cx| {
                            modal.ssh_server_read(ssh_server_of(&state), cx);
                            modal.ssh_checked(
                                !(state.contains("ssh-off")
                                    || state.contains("ssh-missing")
                                    || state.contains("ssh-stopped")
                                    || state.contains("ssh-installing")),
                                cx,
                            );
                            modal.prepared(Ok(()), cx)
                        }),
                    }
                    match state.as_str() {
                        "tailscale" | "tailscale-off" => with_view(&slot, cx, |modal, cx| {
                            modal.switch_way(ConnectWay::Tailscale, cx)
                        }),
                        "large" => with_view(&slot, cx, |modal, cx| modal.set_large_code(true, cx)),
                        "refused-headless" => with_view_window(&slot, cx, |modal, window, cx| {
                            modal.preview_headless_tab(window, cx)
                        }),
                        "computer-ssh-installing" | "computer-ssh-missing-fail" => {
                            with_view(&slot, cx, |modal, cx| modal.preview_install_ssh(cx))
                        }
                        _ => {}
                    }
                })
                .detach();
            }
            ConnectDeviceCommand::EnableSsh { install } => {
                eprintln!("enable ssh install={install}");
                if state == "computer-ssh-installing" {
                    return;
                }
                cx.spawn(async move |cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(if install { 2500 } else { 900 }))
                        .await;
                    let result = if state == "computer-ssh-missing-fail" {
                        Err("Installing the OpenSSH server failed: E: Unable to locate package openssh-server. Install it yourself with `sudo apt install openssh-server` in a terminal, then try again.".to_string())
                    } else if state.contains("ssh-missing") && !install {
                        Err("The OpenSSH server isn't installed on this computer. Install it with `sudo apt install openssh-server`, then try again.".to_string())
                    } else if state.contains("ssh-fail") {
                        Err("Installing the OpenSSH Server feature failed: Add-WindowsCapability failed (0x800f0950).".to_string())
                    } else {
                        Ok(true)
                    };
                    with_view(&slot, cx, |modal, cx| {
                        if result.is_ok() {
                            modal.ssh_server_read(SshServer::default(), cx);
                        }
                        modal.ssh_turned_on(result, cx)
                    });
                })
                .detach();
            }
            ConnectDeviceCommand::InstallHelper => {
                cx.spawn(async move |cx| {
                    let steps = [
                        "Downloading the build tools… 12% of 66 MB",
                        "Downloading the build tools… 64% of 66 MB",
                        "Unpacking the build tools…",
                        "Building the Easy Connect helper. This takes a few minutes…",
                        "Finishing up…",
                    ];
                    for step in steps {
                        with_view(&slot, cx, |modal, cx| {
                            modal.helper_install_progress(Some(step.to_string()), cx)
                        });
                        cx.background_executor()
                            .timer(Duration::from_millis(900))
                            .await;
                    }
                    with_view(&slot, cx, |modal, cx| modal.helper_installed(cx));
                    cx.background_executor()
                        .timer(Duration::from_millis(500))
                        .await;
                    with_view(&slot, cx, |modal, cx| modal.prepared(Ok(()), cx));
                })
                .detach();
            }
            ConnectDeviceCommand::Poll => {
                cx.spawn(async move |cx| {
                    with_view(&slot, cx, |modal, cx| {
                        modal.poll_answered(poll_answer(&state), cx)
                    });
                    if state.starts_with("computer-") && !state.starts_with("computer-ssh-") {
                        with_view_window(&slot, cx, |modal, window, cx| {
                            modal.preview_join(OTHER_CODE, window, cx)
                        });
                    }
                    // `connected` approves the waiting phone, which then shows as paired.
                    if state == "connected" {
                        with_view(&slot, cx, |modal, cx| {
                            modal.decided("approval-1".to_string(), true, Ok(()), cx)
                        });
                        let mut done = poll_answer("code");
                        done.paired_ids.push("mos-iphone".to_string());
                        with_view(&slot, cx, |modal, cx| modal.poll_answered(done, cx));
                    }
                })
                .detach();
            }
            ConnectDeviceCommand::Decide {
                approval_id,
                approve,
            } => {
                eprintln!("decide {approval_id} approve={approve}");
            }
            ConnectDeviceCommand::Copied => eprintln!("copied"),
            ConnectDeviceCommand::OpenExternalUrl(url) => eprintln!("open external url {url}"),
            ConnectDeviceCommand::ReadTailscale => {
                cx.spawn(async move |cx| {
                    let status = serde_json::json!({
                        "computerName": "Mohamad's Laptop",
                        "username": "madda",
                        "platform": "macos",
                        "ssh": { "enabled": true },
                        "tailscale": {
                            "installed": true,
                            "running": true,
                            "account": "madda@example.com",
                            "magicDnsName": "mohamads-laptop.tail1234.ts.net",
                            "ip": "100.64.0.7",
                        },
                    });
                    with_view(&slot, cx, |modal, cx| {
                        modal.tailscale_answered(
                            state != "tailscale-off",
                            tailscale_checklist_of_status(&status),
                            cx,
                        )
                    });
                })
                .detach();
            }
            ConnectDeviceCommand::OpenRemote => {
                eprintln!("open Settings > Remote");
                cx.quit();
            }
            ConnectDeviceCommand::OpenTailscale => {
                eprintln!("open Settings > Remote > Tailscale");
                cx.quit();
            }
            ConnectDeviceCommand::Close => {
                eprintln!("close");
                cx.quit();
            }
            ConnectDeviceCommand::JoinComputer { code } => {
                eprintln!("join {} chars", code.len());
                cx.spawn(async move |cx| {
                    with_view(&slot, cx, |modal, cx| {
                        modal.join_started(
                            Ok((
                                "pd_demo".to_string(),
                                vec!["amber".into(), "falcon".into(), "river".into()],
                                JoinTarget {
                                    name: "MacBook Pro".to_string(),
                                    address: "tcDemo".to_string(),
                                    user: "madda".to_string(),
                                    ssh_port: 22,
                                },
                            )),
                            cx,
                        )
                    });
                })
                .detach();
            }
            ConnectDeviceCommand::JoinStatus { .. } => {
                cx.spawn(async move |cx| {
                    let answer = if state == "computer-words" {
                        serde_json::json!({ "status": "pending" })
                    } else {
                        serde_json::json!({
                            "status": "approved",
                            "sshKeyInstalled": state != "computer-password",
                            "user": "madda",
                        })
                    };
                    with_view(&slot, cx, |modal, cx| modal.join_status(Ok(answer), cx));
                })
                .detach();
            }
            ConnectDeviceCommand::SaveJoined { key_installed, .. } => {
                cx.spawn(async move |cx| {
                    with_view_window(&slot, cx, |modal, window, cx| {
                        modal.join_saved("remote-demo".to_string(), key_installed, window, cx)
                    });
                })
                .detach();
            }
            ConnectDeviceCommand::ReadSignIn { .. } => {
                cx.spawn(async move |cx| {
                    with_view(&slot, cx, |modal, cx| {
                        modal.sign_in_state(Some("connected".to_string()), None, cx)
                    });
                    if state == "computer-both" {
                        with_view(&slot, cx, |modal, cx| modal.preview_connect_back(cx));
                    }
                })
                .detach();
            }
            ConnectDeviceCommand::ConnectBack { .. } => {
                cx.spawn(async move |cx| {
                    for step in [BackStep::Approving, BackStep::Adding] {
                        cx.background_executor()
                            .timer(Duration::from_millis(400))
                            .await;
                        with_view(&slot, cx, |modal, cx| {
                            modal.connect_back_progress(Ok(Some(step)), cx)
                        });
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(400))
                        .await;
                    with_view(&slot, cx, |modal, cx| {
                        modal.connect_back_progress(Ok(None), cx)
                    });
                })
                .detach();
            }
            other => eprintln!(
                "{}",
                match other {
                    ConnectDeviceCommand::AddSshComputer { .. } => "add SSH computer",
                    ConnectDeviceCommand::CancelJoin { .. } => "cancel join",
                    ConnectDeviceCommand::SubmitPassword { .. } => "submit password",
                    ConnectDeviceCommand::Reconnect { .. } => "reconnect",
                    _ => "command",
                }
            ),
        }
    });
    let config = ConnectDeviceConfig {
        palette: demo.palette,
        kind: if demo.state.starts_with("computer") || demo.state.starts_with("refused-") {
            ConnectDeviceKind::Computer
        } else {
            ConnectDeviceKind::Phone
        },
        computer_name: "Mohamad's Laptop".to_string(),
        tailscale_enabled: demo.state != "tailscale-off",
        require_presence: demo.state == "presence",
    };
    let (window, view) = super::open_modal_window(
        CONNECT_DEVICE_MODAL_WIDTH,
        CONNECT_DEVICE_MODAL_INITIAL_HEIGHT,
        move |window, cx| cx.new(|cx| GpuiConnectDeviceWindow::new(config, host, window, cx)),
        cx,
    );
    *slot.borrow_mut() = Some((window, view));
}
