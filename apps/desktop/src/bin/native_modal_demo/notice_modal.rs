//! Preview host for the notice modal: every toast that used to carry a button, in the words the
//! app raises it with. States: `approval-remote` (default), `approval-local`, `account`,
//! `views-off`, `files-off`, `sync-undo`, `chief-undo`, `homebrew`, `remote-copy-path`,
//! `display-scale`, `linux-update`, `terminal-error`, `git-error`, `gxserver-failed`, `queue`
//! (three waiting: "1 of 3"), `copied` (Copy details just pressed). Every click is printed and the
//! queue moves on, as in the app.
use super::notice_modal::*;
use gpui::{App, AppContext as _};
use std::rc::Rc;

fn item(
    id: &str,
    tone: NoticeModalTone,
    title: &str,
    message: &str,
    action: Option<&str>,
    copy: Option<&str>,
) -> NoticeModalItem {
    NoticeModalItem {
        id: id.to_string(),
        tone,
        title: title.to_string(),
        message: Some(message.to_string()),
        source: Vec::new(),
        action_label: action.map(str::to_string),
        copy_text: copy.map(str::to_string),
        hint: None,
    }
}

fn approval(id: &str, message: &str, asker: &str, computer: &str, hint: &str) -> NoticeModalItem {
    NoticeModalItem {
        source: vec![
            ("Asked by".to_string(), asker.to_string()),
            ("Computer".to_string(), computer.to_string()),
        ],
        hint: Some(hint.to_string()),
        ..item(
            id,
            NoticeModalTone::Approval,
            "Approval needed",
            message,
            Some("Approve"),
            None,
        )
    }
}

fn case(state: &str) -> NoticeModalItem {
    use NoticeModalTone::*;
    match state {
        "approval-local" => approval(
            "approval-local",
            "Turn on GBots.",
            "Fix login bug",
            "This computer",
            "Approve asks for Touch ID or your password first.",
        ),
        "account" => item(
            "account",
            Warning,
            "Account unavailable",
            "“Fix login” (Claude) is on Claude account 3 (Work), which was removed from Ghostex.",
            Some("Open Accounts"),
            None,
        ),
        "views-off" => item(
            "views-off",
            Info,
            "No view to show file changes in",
            "Turn on the Changes view or VS Code in Settings > Extensions.",
            Some("Open Settings"),
            None,
        ),
        "files-off" => item(
            "files-off",
            Info,
            "The Files view is turned off",
            "Turn on Files in Settings > Extensions to browse remote projects.",
            Some("Open Settings"),
            None,
        ),
        "sync-undo" => item(
            "sync-undo",
            Info,
            "Changed on studio-mac",
            "Theme is “Light” from studio-mac, replacing “Dark” from this computer.",
            Some("Undo"),
            None,
        ),
        "chief-undo" => item(
            "chief-undo",
            Info,
            "Chief of Staff turned off",
            "Its chat and memory are kept. Turn it back on in Settings > Extensions.",
            Some("Undo"),
            None,
        ),
        "homebrew" => item(
            "homebrew",
            Warning,
            "Lazygit needs lazygit",
            "Lazygit opens the lazygit command in a terminal, and this computer does not have it. Install it with Homebrew below, then open Lazygit again.",
            Some("Install with Homebrew"),
            None,
        ),
        "remote-copy-path" => item(
            "remote-copy-path",
            Warning,
            "Zed can't open a remote project",
            "It only opens folders on this computer. Copy the path to use it on the remote computer.",
            Some("Copy Path"),
            None,
        ),
        "display-scale" => item(
            "display-scale",
            Info,
            "Ghostex follows your display scale",
            "It now draws at your desktop's 150%, so your raised Interface scale makes everything extra large. Set Interface scale to 100%, or close this to keep it.",
            Some("Set to 100%"),
            None,
        ),
        "linux-update" => item(
            "linux-update",
            Warning,
            "Ghostex was not updated",
            "The AppImage could not be replaced: permission denied.",
            Some("Open download page"),
            Some("permission denied (os error 13)"),
        ),
        "terminal-error" | "copied" => item(
            "terminal-error",
            Warning,
            "Couldn't open this terminal",
            "Ghostex couldn't reconnect to it. Try again, or restart it from the sidebar.",
            None,
            Some("attach refused: zmx session gx-4f2a not found"),
        ),
        "git-error" => item(
            "git-error",
            Error,
            "Push didn't finish",
            "Git reported a problem. Copy details has what it said.",
            None,
            Some("! [rejected] main -> main (fetch first)"),
        ),
        "gxserver-failed" => item(
            "gxserver-failed",
            Error,
            "Ghostex couldn't start",
            "It stopped while starting. Quit and reopen Ghostex; Copy details has what went wrong.",
            None,
            Some("gxserver exited with status 1"),
        ),
        _ => approval(
            "approval-remote",
            "Let the agents on studio-mac and this computer reach each other. Approved computers stay connected for 30 days.",
            "Ghostex",
            "studio-mac",
            "Approve asks for Windows Hello first.",
        ),
    }
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let items = if demo.state == "queue" {
        vec![case("approval-remote"), case("account"), case("git-error")]
    } else {
        vec![case(&demo.state)]
    };
    let slot: Rc<std::cell::RefCell<Option<gpui::Entity<GpuiNoticeModalWindow>>>> =
        Default::default();
    let host_slot = slot.clone();
    let host: NoticeModalHost = Rc::new(move |command, cx: &mut App| {
        eprintln!("{command:?}");
        let Some(view) = host_slot.borrow().clone() else {
            return;
        };
        view.update(cx, |view, cx| {
            let mut items = view.items().to_vec();
            items.remove(0);
            if items.is_empty() {
                cx.quit();
            } else {
                view.set_items(items, cx);
            }
        });
    });
    let palette = demo.palette;
    let copied = demo.state == "copied";
    let (_, view) = super::open_modal_window(
        NOTICE_MODAL_WIDTH,
        NOTICE_MODAL_INITIAL_HEIGHT,
        move |window, cx| {
            cx.new(|cx| {
                let mut view = GpuiNoticeModalWindow::new(palette, host, items, window, cx);
                if copied {
                    view.mark_copied();
                }
                view
            })
        },
        cx,
    );
    *slot.borrow_mut() = Some(view);
}
