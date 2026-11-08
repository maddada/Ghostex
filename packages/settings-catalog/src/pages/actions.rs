use crate::rows::{row, section, Page, Section};

pub(crate) fn page() -> Page {
    Page {
        id: "actions",
        title: "Actions",
        sections: vec![actions()],
    }
}

pub(crate) fn actions() -> Section {
    section(
        "actions",
        "Actions",
        vec![
            row("terminalAction", "Terminal Action", "Add terminal actions to run saved commands in quick command terminals with one click or a hotkey."),
            row("browserAction", "Browser Action", "Add browser actions to open saved URLs in browser panes."),
            row("actionShortcuts", "Custom actions", "Actions are custom shortcuts for repeat work, shared between a main project and its worktrees."),
            row("globalActions", "Global Actions", "Global actions apply to every project, are stored by the Ghostex daemon, and appear under Global in every project's Start menu."),
        ],
    )
}
