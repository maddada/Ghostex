use crate::rows::{row, section, Page, Section};

pub(crate) fn page() -> Page {
    Page {
        id: "osIntegration",
        title: "OS Integration",
        sections: vec![defaults(), cli(), diagnostics()],
    }
}

pub(crate) fn defaults() -> Section {
    section(
        "defaults",
        "Defaults",
        vec![
            row(
                "setDefaultEditor",
                "Set as Default Editor",
                "Make Ghostex the default editor for supported file types.",
            ),
            row(
                "setGhostexLinks",
                "Set Ghostex Links",
                "Make Ghostex the handler for ghostex:// links, which open a session from a browser or another app.",
            ),
            row(
                "setScriptRunner",
                "Set Script Runner",
                "Make Ghostex the default script runner.",
            ),
            row(
                "setAll",
                "Set All",
                "Set Ghostex as default editor, ghostex:// link handler, and script runner.",
            ),
        ],
    )
}

pub(crate) fn cli() -> Section {
    section(
        "cli",
        "CLI",
        vec![row(
            "cliCommands",
            "ghostex command line",
            "Command-line examples: ghostex open, ghostex edit, ghostex terminal.",
        )],
    )
}

pub(crate) fn diagnostics() -> Section {
    section(
        "diagnostics",
        "Diagnostics",
        vec![row(
            "handlerStatus",
            "File and link handler status",
            "Check system registration for editor defaults, script runner, and ghostex:// links.",
        )],
    )
}
