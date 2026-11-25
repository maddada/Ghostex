//! The Browser page's search rows (layout.rs `browser`), one function per section.
//!
//! CDXC:Settings 2026-11-26 DECISION:
//! User (Q5021-Q5023, #762): a Settings > Browser page gathers the Browser's settings, right after
//! Files & Links in the rail's Working group. Agents holds the new Let agents open and switch pages
//! without asking (`agentPagesOpenWithoutAsking`) and Agents open pages in; Links holds Open links in
//! and Open session links in; Pages holds Browser pages (Beside the views / In their own row), Reload open pages on launch
//! and Browser sleeps after (both advanced; Power & Sleep keeps Views auto sleep after, which still
//! sets it); Web runtime holds the Chromium runtime (CEF) row from Integrations > Embedded browser;
//! Sign-ins holds this window's workspace's Browser sign-ins and Site permissions (Workspaces keeps
//! them for every workspace). The Grok Bot rows stay on Files & Links (`tools::grok_web`).
//! SEE-ALSO: apps/desktop/src/app/window/settings_modal/tabs/general/browser.rs (the page).
use crate::data::*;
use crate::help::AGENT_PAGES_OPEN_WITHOUT_ASKING_SETTING;
use crate::rows::{row, section, Section};

pub(crate) fn agents() -> Section {
    section(
        "browserAgents",
        "Agents",
        vec![
            row(AGENT_PAGES_OPEN_WITHOUT_ASKING_SETTING, "Let agents open and switch pages without asking", "When an agent opens a page, the Browser shows it right away instead of asking with a toast first. The toast's Always button turns this on."),
            row("agentPreviewBrowser", "Agents open pages in", "Where an agent opens a page it built for you, such as a website preview: the project Browser view or your default browser. Ask has the agent ask you the first time and save your answer here.").options(AGENT_PREVIEW_BROWSER_OPTIONS),
        ],
    )
}

pub(crate) fn links() -> Section {
    section(
        "browserLinks",
        "Links",
        vec![
            row("webLinkOpenTarget", "Open links in", "Open web links from terminal output (Command-click), session chat, and detected dev servers in the project Browser view or the system default browser.").options(WEB_LINK_OPEN_TARGET_OPTIONS),
            row("sessionLinkOpenTarget", "Open session links in", "Open a session's pull requests, issues and Linear tickets (the chips after its title and its links list) in the project Browser view or the system default browser. The phone follows this setting too.").options(SESSION_LINK_OPEN_TARGET_OPTIONS),
        ],
    )
}

pub(crate) fn pages() -> Section {
    section(
        "browserPages",
        "Pages",
        vec![
            row("browserPagesInViewRow", "Browser pages", "Beside the views: each page is a tab beside the views in the side panel's tab bar. In their own row: the Browser is one tab there, with its pages as a row of tabs under it. Pages open, close, sleep and drag as before either way."),
            row("reloadBrowserTabsOnStartup", "Reload open pages on launch", "Automatically reload the open project's browser tabs when Ghostex starts so pages are ready before you click them."),
            row(
                "autoSleepBrowserIdleMinutes",
                "Browser sleeps after",
                "Choose when inactive browser panes sleep, or turn Auto Sleep off. Off also keeps the Browser awake when more than three views are open. Power & Sleep's Views auto sleep after sets it too.",
            )
            .num_options(AUTO_SLEEP_IDLE_MINUTE_OPTIONS),
        ],
    )
}

pub(crate) fn web_runtime() -> Section {
    section(
        "browserRuntime",
        "Web runtime",
        vec![
            row("webRuntime", "Chromium runtime (CEF)", "Install, reinstall or uninstall the optional web runtime used by the Browser, the VS Code view, website and extension views, and HTML files in Files. It also installs from the first view that needs it."),
        ],
    )
}

pub(crate) fn sign_ins() -> Section {
    section(
        "browserSignIns",
        "Sign-ins",
        vec![
            row("browserSignins", "Browser sign-ins", "Sign this window's workspace's Browser out of every site. Each workspace's Browser keeps its own cookies; Settings > Workspaces has the same button for every workspace."),
            row("browserSitePermissions", "Site permissions", "Forget your Allow and Don't Allow answers to sites in this window's workspace's Browser, such as a site connecting to apps on this computer, so each site asks again."),
        ],
    )
}
