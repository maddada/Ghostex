//! The Browser page: Agents, Links, Pages, Web runtime and Sign-ins.
//!
//! CDXC:Settings 2026-11-26 DECISION:
//! User (Q5021-Q5023, #762): a Settings > Browser page gathers the Browser's settings, approved
//! from a mockup. What moved here: Agents open pages in, Open links in, Open session links in, Show
//! pages in their own row and Reload open pages on launch from Files & Links > Browser; Browser
//! sleeps after from Power & Sleep > Auto Sleep (an advanced row; Views auto sleep after stays there
//! and still sets it); the Chromium runtime (CEF) row from Integrations > Embedded browser, which
//! keeps a line pointing here; and Browser sign-ins and Site permissions for this window's
//! workspace, which Workspaces keeps for every workspace. New: Let agents open and switch pages
//! without asking (`agentPagesOpenWithoutAsking`), which the switch toast's Always turns on. The
//! Grok Bot rows stay on Files & Links (chat_tools.rs `grok_web_section`).
//! SEE-ALSO: packages/settings-catalog/src/general/browser.rs (its search rows), model.rs
//! (`SettingsTabId::Browser`).
use super::super::super::fields::{
    ButtonVariant, RowSpec, setting_row, settings_button, settings_section,
};
use super::super::super::page::PageBlock;
use super::super::super::store::post_store_message;
use super::{GeneralCx, GeneralTab, web_runtime};
use ghostex_settings_catalog::help::AGENT_PAGES_OPEN_WITHOUT_ASKING_SETTING;
use gpui::{AnyElement, Context, SharedString, Window};
use serde_json::json;

fn agents_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    let s = "browserAgents";
    if !g.search.subsection_visible(s, g.power_visible) {
        return None;
    }
    let mut rows: Vec<AnyElement> = Vec::new();
    rows.extend(page.toggle(
        g,
        s,
        AGENT_PAGES_OPEN_WITHOUT_ASKING_SETTING,
        "Let agents open and switch pages without asking",
        "When an agent opens a page, the Browser shows it right away instead of asking with a toast first. The toast's Always button turns this on.",
        false,
        cx,
    ));
    rows.extend(page.select(
        g,
        s,
        "agentPreviewBrowser",
        "Agents open pages in",
        "Where an agent opens a page it built for you, such as a website preview: the project Browser view or your default browser. Ask has the agent ask you the first time and save your answer here.",
        g.options("AGENT_PREVIEW_BROWSER_OPTIONS"),
        None,
        false,
        window,
        cx,
    ));
    settings_section(&g.p, g.section_title(s, "Agents"), None, None, rows)
        .map(|section| PageBlock::section(s, section))
}

fn links_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    let s = "browserLinks";
    if !g.search.subsection_visible(s, g.power_visible) {
        return None;
    }
    let mut rows: Vec<AnyElement> = Vec::new();
    rows.extend(page.select(
        g,
        s,
        "webLinkOpenTarget",
        "Open links in",
        "Open web links from terminal output (Command-click), session chat, and detected dev servers in the project Browser view or the system default browser.",
        g.options("WEB_LINK_OPEN_TARGET_OPTIONS"),
        None,
        false,
        window,
        cx,
    ));
    rows.extend(page.select(
        g,
        s,
        "sessionLinkOpenTarget",
        "Open session links in",
        "Open a session's pull requests, issues and Linear tickets (the chips after its title and its links list) in the project Browser view or the system default browser. The phone follows this setting too.",
        g.options("SESSION_LINK_OPEN_TARGET_OPTIONS"),
        None,
        false,
        window,
        cx,
    ));
    settings_section(&g.p, g.section_title(s, "Links"), None, None, rows)
        .map(|section| PageBlock::section(s, section))
}

fn pages_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    let s = "browserPages";
    if !g.search.subsection_visible(s, g.power_visible) {
        return None;
    }
    let mut rows: Vec<AnyElement> = Vec::new();
    rows.extend(page.toggle(
        g,
        s,
        "browserPagesInViewRow",
        "Show pages in their own row",
        "Show the Browser as one tab in the side panel's tab bar, with its pages as a row of tabs under it, instead of each page as a tab beside the views.",
        false,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "reloadBrowserTabsOnStartup",
        "Reload open pages on launch",
        "Automatically reload the open project's browser tabs when Ghostex starts so pages are ready before you click them.",
        false,
        cx,
    ));
    rows.extend(page.number_select(
        g,
        s,
        "autoSleepBrowserIdleMinutes",
        "Browser sleeps after",
        "Choose when inactive browser panes sleep, or turn Auto Sleep off. Off also keeps the Browser awake when more than three views are open. Power & Sleep's Views auto sleep after sets it too.",
        g.options("AUTO_SLEEP_IDLE_MINUTE_OPTIONS"),
        false,
        window,
        cx,
    ));
    settings_section(&g.p, g.section_title(s, "Pages"), None, None, rows)
        .map(|section| PageBlock::section(s, section))
}

/// The runtime lives on this computer; the row and its status are web_runtime.rs.
fn web_runtime_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    let s = "browserRuntime";
    if !g.search.subsection_visible(s, g.power_visible) || !g.visible(s, "webRuntime") {
        return None;
    }
    let rows = vec![page.web_runtime_status_row(&g.p, cx)];
    settings_section(
        &g.p,
        g.section_title(s, "Web runtime"),
        Some(SharedString::from(web_runtime::DESCRIPTION)),
        None,
        rows,
    )
    .map(|section| PageBlock::section(s, section))
}

/// This window's workspace's Browser sign-ins and site answers, with the Workspaces page's own
/// actions (`clearWorkspaceBrowserSignins`, `forgetWorkspaceBrowserSiteAnswers`). As there, the
/// default workspace has no Sign out of all sites: it uses the Default Browser profile
/// (app/workspace_browser.rs).
fn sign_ins_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    let s = "browserSignIns";
    if !g.search.subsection_visible(s, g.power_visible) {
        return None;
    }
    let workspace_id = page.store.read(cx).request().window_workspace_id.clone();
    let mut rows: Vec<AnyElement> = Vec::new();
    if let Some(target) = workspace_id.clone()
        && g.visible(s, "browserSignins")
    {
        rows.push(setting_row(
            &g.p,
            "browserSignins",
            RowSpec::new("Browser sign-ins").description(
                "This window's workspace's Browser keeps its own cookies and history, so you can be signed in to a different GitHub or Linear account here. Settings > Workspaces has this for every workspace.",
            ),
            None,
            settings_button(
                &g.p,
                "browser-signout",
                "Sign out of all sites",
                None,
                ButtonVariant::Outline,
                false,
                None,
                move |page: &mut GeneralTab, _window, cx| {
                    post_store_message(
                        &page.store,
                        json!({ "type": "clearWorkspaceBrowserSignins", "workspaceId": target }),
                        cx,
                    );
                },
                cx,
            ),
            cx,
        ));
    }
    if g.visible(s, "browserSitePermissions") {
        rows.push(setting_row(
            &g.p,
            "browserSitePermissions",
            RowSpec::new("Site permissions").description(
                "Your Allow and Don't Allow answers to sites in this window's workspace's Browser, such as letting linear.app connect to apps on this computer. Forget them so each site asks again.",
            ),
            None,
            settings_button(
                &g.p,
                "browser-site-answers-button",
                "Forget all answers",
                None,
                ButtonVariant::Outline,
                false,
                None,
                move |page: &mut GeneralTab, _window, cx| {
                    post_store_message(
                        &page.store,
                        json!({ "type": "forgetWorkspaceBrowserSiteAnswers", "workspaceId": workspace_id }),
                        cx,
                    );
                },
                cx,
            ),
            cx,
        ));
    }
    settings_section(&g.p, g.section_title(s, "Sign-ins"), None, None, rows)
        .map(|section| PageBlock::section(s, section))
}

/// The Browser page (CDXC:Settings 2026-11-26 above).
pub(super) fn sections(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Vec<PageBlock> {
    [
        agents_section(page, g, window, cx),
        links_section(page, g, window, cx),
        pages_section(page, g, window, cx),
        web_runtime_section(page, g, cx),
        sign_ins_section(page, g, cx),
    ]
    .into_iter()
    .flatten()
    .collect()
}
