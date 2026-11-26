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
                "executorCli",
                "Executor CLI",
                "The Executor CLI installed on this computer and its one sign-in, which every workspace's agents share: install it, sign in to executor.sh or a self-hosted Executor, or sign out.",
            ),
            row(
                "executorApps",
                "Executor apps",
                "The apps the Executor CLI's sign-in reaches; add apps and their accounts in Executor itself.",
            ),
            row(
                "executorOldApps",
                "Apps on the old Executor",
                "The apps Ghostex's own Executor had, listed until you remove their data or 30 days pass; add them again in Executor.",
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
