use crate::hotkey_label::hotkey_label;
use crate::rows::{row, section, Page, Section};
use crate::Platform;

/// CDXC:Extensions 2026-08-30:
/// One Extensions page covers the built-in features (searchable by their own
/// names, straight from the shared descriptor list) and the extension store.
pub(crate) fn page(platform: Platform) -> Page {
    Page {
        id: "extensions",
        title: "Extensions",
        sections: vec![
            view_order(platform),
            official(platform),
            store(),
            custom_views(),
        ],
    }
}

pub(crate) fn view_order(platform: Platform) -> Section {
    section(
        "viewOrder",
        "Views",
        vec![
            row("titlebarViewOrder", "Arrange views", format!("Reorder built-in, extension, and custom views. New tabs open in this order, and {} through {} follow the tabs in the view panel.", hotkey_label("alt+1", platform), hotkey_label("alt+9", platform))),
        ],
    )
}

pub(crate) fn official(platform: Platform) -> Section {
    let mut rows = vec![
        row("linear", "Linear", "Open your team's issues and projects. Choose a home URL for each project or worktree."),
        row("jira", "Jira", "Keep your team's board beside your work. Choose a home URL for each project or worktree."),
        row("github", "GitHub", "Open this project's GitHub repository automatically from its origin remote."),
        row("sentry", "Sentry", "Investigate errors and stack traces beside your code. Choose a home URL for each project or worktree."),
        row("figma", "Figma", "Keep designs and component specs beside your implementation. Choose a home URL for each project or worktree."),
        row("vercel", "Vercel", "Check deployments and preview your changes. Choose a home URL for each project or worktree."),
        row("supabase", "Supabase", "Browse your database, authentication, and project logs. Choose a home URL for each project or worktree."),
        row("github-actions", "GitHub Actions", "Follow workflow runs, build results, and job logs. Choose a home URL for each project or worktree."),
        row("posthog", "PostHog", "Explore product analytics and session replays. Choose a home URL for each project or worktree."),
        row("custom-website", "Custom Website", "Keep any website beside your work. Choose a home URL for each project or worktree."),
        row("storybook", "Storybook", "Build, browse, and annotate your project’s components without a persistent development server. Appears only in projects with Storybook."),
        row("code", "VS Code", "Explore, edit, and search your project in a familiar, full-featured workspace without ever leaving Ghostex."),
        row("browser", "Browser", "Open websites alongside your project and keep useful pages organized without leaving Ghostex."),
        row("kanban", "Kanban", "Plan upcoming work and track task progress at a glance."),
        row("automate", "Automate", "Turn repeatable project routines into simple workflows you can run whenever you need them."),
        row("bots", "Bots", "Swap the sidebar to your Hermes agents: one row each, with a new session one click away. Needs the Hermes CLI."),
        row("botAutomations", "Bot automations", "A feed of every Hermes cron run, one channel per job, opened from the Bots sidebar. Needs Bots."),
        row("gbots", "GBots", "Your own team of bots under Chief of Staff in Main Hub, each with its own instructions and memory, running on your Claude Code or Codex. Needs Main Hub."),
        row("docs", "Files", "Browse your project’s notes, plans, and reference files together in one focused reading space."),
        row("changes", "Changes", "Review what changed, comment on lines for your agent, revert a hunk, and commit, all beside your sessions."),
        row("terminal", "Terminal", "A command terminal beside your sessions, with its own tabs and splits, that works like the Commands pane but lives in the view panel."),
        row("notifications", "Notifications", "A bell in the sidebar's top row that lists what your agents finished or need from you."),
        row("help", "Ghostex Help", "An entry under Help in the sidebar's menu, with sample questions that start a Ghostex Help chat: an agent explains the app or changes settings for you."),
        row("devServers", "Dev servers", "A panel listing development servers running on this computer, opened from Tools in the sidebar's menu."),
        row("resources", "Resources", "A panel listing what Ghostex is running right now, with the CPU and memory each part is using, opened from Tools in the sidebar's menu."),
        row("gitActions", "Git actions", "A work area header button for commit, branch, and worktree helpers on the active project."),
        row("quickActions", "Actions", "Saved terminal commands and web pages you start in one click from the Start button, a hotkey, Quick Access or a project row."),
        row("openIn", "Open In", "Open the active project or a session's folder in your editor, terminal or file manager from the Open button and the Open In menus."),
        row("cloudBoxes", "Cloud Boxes", "Run agent sessions on a Cloud Box: a Linux machine on your Instabox or Boat account, started from a new chat, with more providers through agentbox."),
        row("workspaces", "Workspaces", "Keep each company's projects, Spaces, browser sign-ins, Linear key and Claude account in its own workspace, pick each one's primary tracker, and share a team backend for your team's Slack flow."),
        row("connectors", "Connectors", "Connect Gmail, Slack, Linear and more apps to a workspace so its agents can use them, from the Connectors page and the ghostex connectors commands."),
        row("osIntegration", "OS Integration", "Make Ghostex your default editor, terminal-link handler and script runner, and check how this computer opens its files and links."),
        row("debugging", "Debugging", "The Debugging page in Settings: debug UI controls and the diagnostic logs you turn on while reproducing an issue."),
        row("extensionsButton", "Extensions", "An entry under Tools in the sidebar's menu that lists your installed extensions."),
    ];
    rows.retain(|row| crate::built_in_extensions::available_on(row.key, platform));
    section("official", "Built-in", rows)
}

pub(crate) fn store() -> Section {
    section(
        "store",
        "Get more",
        vec![row(
            "store",
            "Get more extensions",
            "Browse audited extensions, install them, and manage what is already installed.",
        )],
    )
}

pub(crate) fn custom_views() -> Section {
    section(
        "customViews",
        "Your views",
        vec![
            row("customViews", "Your views", "Project views, templates, Storybook, Linear, GitHub Issues, dev server commands, and HTML reports."),
        ],
    )
}
