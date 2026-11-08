//! Preview host for the 11.0.1 security notice. States: `mixed` (default: phones waiting, one
//! computer paired again), `ticked` (a phone paired again too), `no-code` (Easy Connect off),
//! `computers-only`.
use super::security_upgrade_modal::*;
use gpui::{App, AppContext as _};
use std::rc::Rc;

fn row(id: &str, name: &str, detail: &str, paired: bool) -> SecurityNoticeRow {
    SecurityNoticeRow {
        id: id.to_string(),
        name: name.to_string(),
        detail: detail.to_string(),
        status: if paired {
            SecurityNoticeRowStatus::Paired
        } else {
            SecurityNoticeRowStatus::Waiting
        },
        words: Vec::new(),
        approval_id: None,
    }
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let ticked = demo.state == "ticked";
    let phones = if demo.state == "computers-only" {
        Vec::new()
    } else {
        vec![
            row("a", "iPhone", "iPhone \u{b7} used 2 hours ago", ticked),
            row("b", "iPhone", "iPhone \u{b7} used 6 days ago", false),
            row(
                "c",
                "Pixel 8",
                "Android phone \u{b7} used 12 days ago",
                false,
            ),
        ]
    };
    let computers = vec![
        row(
            "m1",
            "Studio",
            "Easy Connect \u{b7} connected 3 hours ago",
            false,
        ),
        row(
            "m2",
            "Omarchy",
            "Easy Connect \u{b7} connected yesterday",
            true,
        ),
    ];
    let code = if demo.state == "no-code" {
        SecurityNoticeCode::Unavailable(
            "Turn on Easy Connect in Settings \u{203a} Remote to pair phones.".to_string(),
        )
    } else {
        SecurityNoticeCode::Ready(
            "ghostex://pair?v=2&a=tc1q8v3k2m9x7p4r6t8w1y5z2a4c6e8g0j3l5n7q9s1u3w5y7a9c1e3g5i7k9m1&n=Studio&u=madda&p=58744&s=22&nonce=Jr7h2Qm9xLw4".to_string(),
        )
    };
    let state = SecurityUpgradeModalState {
        phones,
        computers,
        code,
    };
    let host: SecurityUpgradeModalHost = Rc::new(move |command, cx: &mut App| {
        match command {
            SecurityUpgradeModalCommand::PairComputer(id) => eprintln!("pair computer {id}"),
            SecurityUpgradeModalCommand::OpenRemoteSettings => eprintln!("open remote settings"),
            SecurityUpgradeModalCommand::Later => eprintln!("later"),
            SecurityUpgradeModalCommand::Done => eprintln!("done"),
            SecurityUpgradeModalCommand::ApprovePending(id) => eprintln!("approve {id}"),
        }
        cx.quit();
    });
    let palette = demo.palette;
    super::open_modal_window(
        SECURITY_UPGRADE_MODAL_WIDTH,
        SECURITY_UPGRADE_MODAL_INITIAL_HEIGHT,
        move |window, cx| {
            cx.new(|cx| GpuiSecurityUpgradeModalWindow::new(palette, state, host, window, cx))
        },
        cx,
    );
}
