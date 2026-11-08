use crate::rows::{row, section, Page, Section};

/// Settings › Connectors, shown only while its switch in Settings > Extensions > Experimental is on.
pub(crate) fn page() -> Page {
    Page {
        id: "connectors",
        title: "Connectors",
        sections: vec![composio(), executor(), rules(), cli_sign_ins()],
    }
}

pub(crate) fn composio() -> Section {
    section(
        "composio",
        "Composio",
        vec![
            row(
                "composioSignIn",
                "Composio sign-in",
                "Sign a workspace in to Composio once in your browser, or sign it out.",
            ),
            row(
                "composioApps",
                "Composio apps",
                "Connect Gmail, Google Calendar, Slack, Notion, GitHub, Linear and more apps for a workspace's agents.",
            ),
        ],
    )
}

pub(crate) fn executor() -> Section {
    section(
        "executor",
        "Executor",
        vec![
            row(
                "executorEnabled",
                "Executor",
                "Turn on Executor for a workspace; Ghostex downloads it the first time and runs it on this computer.",
            ),
            row(
                "executorApps",
                "Executor apps",
                "Add apps from Executor's catalog or any remote MCP server, sign them in and remove them.",
            ),
            row(
                "executorCallbackUrl",
                "Sign-in callback address",
                "The address to give an app's developer console when it needs your own sign-in app (OAuth client).",
            ),
        ],
    )
}

pub(crate) fn rules() -> Section {
    section(
        "rules",
        "When agents change something",
        vec![
            row(
                "connectorRulesDefault",
                "Ask, Allow or Never",
                "What a workspace does before an agent sends, posts, creates, changes or deletes something through a connector: ask on a card, allow it, or never run it.",
            ),
            row(
                "connectorRulesTools",
                "Rules for one tool",
                "The tools set to always allow or never run, for example after Always allow on a card.",
            ),
        ],
    )
}

pub(crate) fn cli_sign_ins() -> Section {
    section(
        "cli",
        "Command-line sign-ins",
        vec![row(
            "connectorCliSignIns",
            "GitHub CLI and Google Workspace CLI",
            "Each workspace's own gh and gws sign-in, which agents started there use.",
        )],
    )
}
