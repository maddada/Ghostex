//! Preview host for the pair-again notice. States: `list` (default), `phone-code`, `phone-words`,
//! `phone-paired`, `ssh-waiting`, `ssh-words`, `ssh-paired`, `computer-paste`, `computer-waiting`,
//! `computer-failed`, `computer-paired`, `ssh-computer`, `list-partway`, `all-paired`, `no-code`,
//! `saved` (the phones of a real answer in GHOSTEX_NATIVE_MODAL_DEMO_NOTICE; the
//! `GHOSTEX_NATIVE_MODAL_DEMO_FORGET=<row id>` asks that row's "Forget it?").
use super::security_upgrade_modal::*;
use gpui::{App, AppContext as _};
use std::rc::Rc;

fn row(
    id: &str,
    name: &str,
    kind_label: &str,
    device: NoticeDevice,
    status: SecurityNoticeRowStatus,
    words: &[&str],
) -> SecurityNoticeRow {
    let pending = status == SecurityNoticeRowStatus::Pending;
    SecurityNoticeRow {
        id: id.to_string(),
        name: name.to_string(),
        kind_label: kind_label.to_string(),
        device,
        pending_id: pending.then(|| format!("pd_{id}")),
        detail: String::new(),
        status,
        words: words.iter().map(|word| word.to_string()).collect(),
        approval_id: pending.then(|| format!("apr_{id}")),
    }
}

/// A real gxserver's `/api/securityUpgradeNotice` answer saved to the file named by
/// `GHOSTEX_NATIVE_MODAL_DEMO_NOTICE`, to preview what a seeded record shows.
pub(super) fn saved_notice() -> Option<serde_json::Value> {
    let path = std::env::var("GHOSTEX_NATIVE_MODAL_DEMO_NOTICE").ok()?;
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// The saved answer's Easy Connect phone rows, as the app reads them (gx_store/security_notice.rs).
fn saved_phone_rows(view: &serde_json::Value) -> Vec<SecurityNoticeRow> {
    view["phones"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|phone| {
            let text = |key: &str| phone[key].as_str().unwrap_or_default().to_string();
            let kind = text("kind");
            let status = match phone["status"].as_str() {
                Some("paired") => SecurityNoticeRowStatus::Paired,
                Some("leftover") => SecurityNoticeRowStatus::Leftover,
                _ => SecurityNoticeRowStatus::Waiting,
            };
            let mut row = row(
                &text("id"),
                &text("name"),
                &if status == SecurityNoticeRowStatus::Leftover {
                    format!("{kind} \u{b7} {}", leftover_reason(&text("leftover")))
                } else {
                    format!("{kind} \u{b7} Easy Connect")
                },
                NoticeDevice::EasyConnectPhone,
                status,
                &[],
            );
            row.status = status;
            row
        })
        .collect()
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    use NoticeDevice::*;
    use SecurityNoticeRowStatus::*;
    let state = demo.state.as_str();
    if state == "saved" {
        return open_saved(demo, cx);
    }
    let all = state == "all-paired";
    let iphone = match state {
        "phone-words" => Pending,
        "phone-paired" | "list-partway" | "all-paired" => Paired,
        _ => Waiting,
    };
    let ssh = match state {
        "ssh-waiting" => Waiting,
        "ssh-paired" | "list-partway" | "all-paired" => Paired,
        _ => Pending,
    };
    let phones = vec![
        row(
            "a",
            "iPhone",
            "iPhone \u{b7} Easy Connect",
            EasyConnectPhone,
            iphone,
            &["harbor", "cotton", "falcon"],
        ),
        row(
            "b",
            "Pixel 8",
            "Android phone \u{b7} Easy Connect",
            EasyConnectPhone,
            if all { Paired } else { Waiting },
            &[],
        ),
    ];
    let ssh_phones = vec![row(
        SSH_PHONES_ROW_ID,
        if ssh == Waiting {
            "Your phone"
        } else {
            "iPhone 16"
        },
        if ssh == Waiting {
            "Phone \u{b7} Tailscale or SSH"
        } else {
            "iPhone \u{b7} Tailscale or SSH"
        },
        SshPhone,
        ssh,
        &["maple", "tunnel", "violet"],
    )];
    let computers = vec![row(
        "m1",
        "Studio",
        "Computer \u{b7} Easy Connect",
        EasyConnectComputer,
        if all { Paired } else { Waiting },
        &[],
    )];
    let ssh_computers = vec![row(
        "m3",
        "Build box",
        "Computer \u{b7} SSH",
        SshComputer,
        Info,
        &[],
    )];
    let pairing = match state {
        "computer-waiting" | "list-partway" => Some(NoticeComputerPairing::Waiting {
            words: vec!["copper".into(), "island".into(), "whale".into()],
            computer: "Studio".into(),
        }),
        "computer-failed" => Some(NoticeComputerPairing::Failed(
            "Studio denied this computer. Show a new code there and try again.".into(),
        )),
        "computer-paired" => Some(NoticeComputerPairing::Paired),
        _ => None,
    };
    let (code, code_text) = if state == "no-code" {
        (
            SecurityNoticeCode::Unavailable(
                "Turn on Easy Connect in Settings \u{203a} Remote to pair phones.".to_string(),
            ),
            None,
        )
    } else {
        (
            SecurityNoticeCode::Ready(
                "https://ghostex.dev/pair#ghostex-ec1:eyJ2IjoxLCJhIjoidGMxcTh2M2sybTl4N3A0cjZ0OHcxeTV6MmE0YzZlOGcwajNsNW43cTlzMXUzdzV5N2E5YzFlM2c1aTdrOW0xIiwibiI6IlN0dWRpbyIsInUiOiJtYWRkYSIsInAiOjU4NzQ0LCJzIjoiSnI3aDJRbTl4THc0In0".to_string(),
            ),
            Some("ghostex-ec1:eyJ2IjoxfQ".to_string()),
        )
    };
    let state = SecurityUpgradeModalState {
        phones,
        computers,
        ssh_phones,
        ssh_computers,
        code,
        code_text,
        computer_pairing: pairing
            .map(|pairing| vec![("m1".to_string(), pairing)])
            .unwrap_or_default(),
    };
    let page = match demo.state.as_str() {
        "phone-code" | "phone-words" | "phone-paired" | "no-code" => Some("a"),
        "ssh-waiting" | "ssh-words" | "ssh-paired" => Some(SSH_PHONES_ROW_ID),
        "computer-paste" | "computer-waiting" | "computer-failed" | "computer-paired" => Some("m1"),
        "ssh-computer" => Some("m3"),
        _ => None,
    };
    let host: SecurityUpgradeModalHost = Rc::new(move |command, cx: &mut App| {
        match command {
            SecurityUpgradeModalCommand::PairComputer { machine_id, .. } => {
                eprintln!("pair computer {machine_id}")
            }
            SecurityUpgradeModalCommand::OpenRemoteSettings => eprintln!("open remote settings"),
            SecurityUpgradeModalCommand::Later => eprintln!("later"),
            SecurityUpgradeModalCommand::Done => eprintln!("done"),
            SecurityUpgradeModalCommand::ApprovePending(id) => eprintln!("approve {id}"),
            SecurityUpgradeModalCommand::DenyPending(id) => eprintln!("deny {id}"),
            // The page asks for the code as it opens; the preview already has one.
            SecurityUpgradeModalCommand::ShowCode | SecurityUpgradeModalCommand::HideCode => {
                return;
            }
            SecurityUpgradeModalCommand::Forget(id) => {
                eprintln!("forget {id}");
                return;
            }
        }
        cx.quit();
    });
    let palette = demo.palette;
    super::open_modal_window(
        SECURITY_UPGRADE_MODAL_WIDTH,
        SECURITY_UPGRADE_MODAL_INITIAL_HEIGHT,
        move |window, cx| {
            cx.new(|cx| {
                let mut modal =
                    GpuiSecurityUpgradeModalWindow::new(palette, state, host, window, cx);
                if let Some(page) = page {
                    modal.open_page(page, cx);
                }
                modal
            })
        },
        cx,
    );
}

fn open_saved(demo: &super::DemoEnv, cx: &mut App) {
    let view = saved_notice().unwrap_or_default();
    let state = SecurityUpgradeModalState {
        phones: saved_phone_rows(&view),
        computers: Vec::new(),
        ssh_phones: Vec::new(),
        ssh_computers: Vec::new(),
        code: SecurityNoticeCode::Hidden,
        code_text: None,
        computer_pairing: Vec::new(),
    };
    let forget = std::env::var("GHOSTEX_NATIVE_MODAL_DEMO_FORGET").ok();
    let host: SecurityUpgradeModalHost = Rc::new(move |command, cx: &mut App| match command {
        SecurityUpgradeModalCommand::Forget(id) => eprintln!("forget {id}"),
        SecurityUpgradeModalCommand::Later | SecurityUpgradeModalCommand::Done => cx.quit(),
        _ => {}
    });
    let palette = demo.palette;
    super::open_modal_window(
        SECURITY_UPGRADE_MODAL_WIDTH,
        SECURITY_UPGRADE_MODAL_INITIAL_HEIGHT,
        move |window, cx| {
            cx.new(|cx| {
                let mut modal =
                    GpuiSecurityUpgradeModalWindow::new(palette, state, host, window, cx);
                if let Some(id) = forget {
                    modal.ask_forget(&id, cx);
                }
                modal
            })
        },
        cx,
    );
}
