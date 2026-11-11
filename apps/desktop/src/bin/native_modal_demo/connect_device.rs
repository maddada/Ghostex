//! Preview of the Connect a phone window. States: `code` (default: the QR, waiting for a phone),
//! `preparing` (turning SSH access on, never answers), `failed` (turning on failed), `ask` (a phone
//! scanned: its three words and Approve), `connected` (after Approve), `computer` (Connect a
//! computer's code), `presence` (`ask` with Approve asking for Touch ID first), `helper` (the
//! Easy Connect helper isn't installed: Install runs a pretend install, then the code shows),
//! `helper-failed` (its install failed), `helper-installing` (an install that already runs),
//! `large` (the phone's code filling the window, as after a click on it), `tailscale` (the
//! Tailscale way with its checklist), `tailscale-off` (the way while Tailscale is switched off).
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

fn poll_answer(state: &str) -> ConnectPoll {
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
                                "Turning on SSH access. Your computer may ask for your password.".to_string(),
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
                        _ => with_view(&slot, cx, |modal, cx| modal.prepared(Ok(()), cx)),
                    }
                    match state.as_str() {
                        "tailscale" | "tailscale-off" => with_view(&slot, cx, |modal, cx| {
                            modal.switch_way(ConnectWay::Tailscale, cx)
                        }),
                        "large" => with_view(&slot, cx, |modal, cx| modal.set_large_code(true, cx)),
                        _ => {}
                    }
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
        }
    });
    let config = ConnectDeviceConfig {
        palette: demo.palette,
        kind: if demo.state == "computer" {
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
