use crate::data::*;
use crate::json::opt;
use crate::rows::{row, section, Section};

/// CDXC:Extensions 2026-08-30:
/// The built-in view switches are owned by Settings → Extensions and are
/// searchable from that page's own definitions. General no longer claims
/// them, so a query like "kanban" lands on Extensions instead of matching a
/// General page that has no such section to scroll to.
pub(crate) fn file_opening() -> Section {
    section(
        "fileOpening",
        "File opening",
        vec![
            row("documentFileOpenTarget", "HTML, Markdown and images", "Choose where HTML, Markdown and picture files open from Quick Access, agent chat, the terminal and `ghostex open`: Files, VS Code, the system app (your computer's default app for that file type), or an editor app installed on this computer.").option_list(crate::file_opening::file_open_target_options()),
            row("otherFileOpenTarget", "Other files", "Choose where every other file (code and text) opens from Quick Access, agent chat, the terminal and `ghostex open`: Files, VS Code, the system app (your computer's default app for that file type), or an editor app installed on this computer.").option_list(crate::file_opening::file_open_target_options()),
            row("videoFileOpenTarget", "Videos", "Choose whether videos linked in agent chat or the terminal, or opened with `ghostex open`, play in Files or open in the system app. Formats Files cannot play, such as .mp4 and .mov, always open in the system app.").options(MEDIA_FILE_OPEN_TARGET_OPTIONS),
            row("fileChangesOpenTarget", "Open file changes in", "Choose where Review all on an agent's file changes and Session details' Changes card open: the Changes view or the VS Code view. If the chosen view is turned off, the other one opens.").options(FILE_CHANGES_OPEN_TARGET_OPTIONS),
            row("changesHideTestFiles", "Hide test files in Changes", "Leave test files (such as foo_test.go, button.test.ts, FooTests.swift and anything in a tests folder) out of the Changes view, so you can review an agent's change without its tests. A line at the top says how many are hidden, with a click to show them. The flask button in the Changes toolbar switches this too."),
            row("audioFileOpenTarget", "Audio", "Choose whether audio files linked in agent chat or the terminal, or opened with `ghostex open`, play in Files or open in the system app. Formats Files cannot play, such as .m4a, always open in the system app.").options(MEDIA_FILE_OPEN_TARGET_OPTIONS),
            row("docsOpenFilesAsTabs", "Show open files as tabs", "Show the files open in the Files view as a row of tabs above the document, instead of the Open Files list in the files sidebar. Every file you open, from the files list, Quick Access or a chat link, gets a tab, and each keeps its own unsaved changes and notes."),
        ],
    )
}

pub(crate) fn browser() -> Section {
    section(
        "browser",
        "Browser",
        vec![
            row("webLinkOpenTarget", "Open links in", "Open web links from terminal output (Command-click), session chat, and detected dev servers in the project Browser view or the system default browser.").options(WEB_LINK_OPEN_TARGET_OPTIONS),
            row("agentPreviewBrowser", "Agents open pages in", "Where an agent opens a page it built for you, such as a website preview: the project Browser view or your default browser. Ask has the agent ask you the first time and save your answer here.").options(AGENT_PREVIEW_BROWSER_OPTIONS),
            row("sessionLinkOpenTarget", "Open session links in", "Open a session's pull requests, issues and Linear tickets (the chips after its title and its links list) in the project Browser view or the system default browser. The phone follows this setting too.").options(SESSION_LINK_OPEN_TARGET_OPTIONS),
            row("browserPagesInViewRow", "Show pages in their own row", "Show the Browser as one tab in the side panel's tab bar, with its pages as a row of tabs under it, instead of each page as a tab beside the views. Pages open, close, sleep and drag as before."),
            row("grokWebStartPage", "Grok start page", "The page the Grok view opens on, for every Grok login: grok.com, or a page of your own such as one of your Grok bots. Left empty, or not a web address, it opens grok.com. Shown while the Grok extension is on."),
            row("grokWebLogins", "Grok logins", "The Grok logins the bar above the Grok page switches between, each with its own sign-in. Add one there or here; removing one signs it out. Shown while the Grok extension is on."),
        ],
    )
}

pub(crate) fn editor() -> Section {
    section(
        "editor",
        "Editor",
        vec![
            row("codeServerLinkVscodeUserConfig", "Use VS Code settings", "Use the VS Code settings from the local VS Code install."),
            row("codeServerUseVscodeInsidersUserConfig", "Use VS Code Insiders settings", "Use the VS Code Insiders user settings directory."),
            row("codeServerNewWindowsAsTabs", "Open VS Code windows as tabs", "When VS Code opens a new window (File > New Window, or a folder or workspace opened in a new window), show it as another window tab above the VS Code page instead of blocking it. Each tab is its own VS Code window, so you can keep several projects or workspaces open at once."),
        ],
    )
}

pub(crate) fn terminal_dev_servers() -> Section {
    section(
        "terminalDevServers",
        "Dev Servers",
        vec![
            row(
                "terminalDevServerDetectionEnabled",
                "Detect running servers in terminals",
                "Detect localhost dev server URLs from terminal output.",
            ),
            row(
                "terminalDevServerIgnoredPortRules",
                "Ignored ports",
                "Hide detected servers on specific ports or inclusive port ranges.",
            )
            .options(&[opt("9229", "9229"), opt("24678-24680", "24678-24680")]),
        ],
    )
}
