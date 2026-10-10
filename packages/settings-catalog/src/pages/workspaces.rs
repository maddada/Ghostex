use crate::rows::{row, section, Page, Section};

pub(crate) fn page() -> Page {
    Page {
        id: "workspaces",
        title: "Workspaces",
        sections: vec![workspaces(), team(), team_flow()],
    }
}

/// CDXC:Workspaces 2026-10-09 DECISION:
/// User (mockup 09): one Settings page for the workspaces, each with its name, color, kind (Work
/// or Personal, which sets the work-mode default), its Linear API key, the Claude account its
/// agents use and its own browser sign-ins.
pub(crate) fn workspaces() -> Section {
    section(
        "workspaces",
        "Workspaces",
        vec![
            row(
                "workspaceName",
                "Workspace name",
                "The name and letter shown on the workspace button left of your Spaces.",
            ),
            row(
                "workspaceColor",
                "Workspace color",
                "The color of the workspace button.",
            ),
            row(
                "workspaceKind",
                "Work or Personal",
                "Work turns work mode on for the workspace's projects by default; Personal leaves it off.",
            ),
            row(
                "workspaceTracker",
                "Primary tracker",
                "Linear tickets & projects, or GitHub issues & projects: what Create ticket, the Work view, Link to and Slack use. Shows the command that lets Ghostex read GitHub Projects when it can't yet.",
            ),
            row(
                "workspaceLinearApiKey",
                "Linear API key",
                "The Linear key this workspace's projects use, unless a project sets its own.",
            ),
            row(
                "workspaceClaudeAccount",
                "Claude account",
                "Which of your Claude accounts agents in this workspace's projects use.",
            ),
            row(
                "workspaceBrowserSignins",
                "Browser sign-ins",
                "Each workspace's Browser keeps its own cookies; sign out of every site here.",
            ),
            row(
                "workspaceSitePermissions",
                "Site permissions",
                "Forget your Allow and Don't Allow answers to sites in a workspace's Browser, such as a site connecting to apps on this computer.",
            ),
            row(
                "newWorkspace",
                "New workspace",
                "Add a workspace, for example one per company you work for.",
            ),
        ],
    )
}

/// A Work workspace's team: its own Convex project, the team's Slack app and the team-wide Linear
/// key, each with the `ghostex team` command for the parts that need a terminal.
pub(crate) fn team() -> Section {
    section(
        "team",
        "Team",
        vec![
            row(
                "teamConvex",
                "Team (Convex)",
                "Join your team's Convex project with an invite link, copy an invite link for a teammate, or leave the team.",
            ),
            row(
                "teamSetup",
                "Set up a new team",
                "Deploy Ghostex's functions to your own Convex project with ghostex team deploy in a terminal.",
            ),
            row(
                "slackAppManifest",
                "Slack app manifest",
                "Copy the manifest that creates your team's Slack app.",
            ),
            row(
                "slackSecrets",
                "Slack bot token and signing secret",
                "Stored in your team's Convex project with ghostex team slack-connect.",
            ),
            row(
                "slackUser",
                "Your Slack user",
                "Your Slack member ID, so @Ghostex commands you send in Slack reach this computer.",
            ),
            row(
                "teamLinearKey",
                "Linear for the team",
                "The team-wide Linear key Slack commands use to find and create tickets. Only the team's owners set or remove it; tickets created with it show the key's owner as the creator in Linear.",
            ),
            row(
                "teamOwnLinearKey",
                "Create my Slack tickets with my own Linear key",
                "Tickets you request from Slack are created with this workspace's Linear key, so Linear shows you as their creator.",
            ),
        ],
    )
}

/// A Work workspace's team flow: the Slack flow settings kept in the team's Convex project, and the
/// steps the Work page shows on each ticket.
pub(crate) fn team_flow() -> Section {
    section(
        "teamFlow",
        "Team flow",
        vec![
            row(
                "teamWorkingChannel",
                "Working channel",
                "The Slack channel where each ticket gets its one working thread.",
            ),
            row(
                "teamWatchOnlyChannels",
                "Watch-only channels",
                "Slack channels where a request is forwarded to the ticket's working thread and only gets a 👀.",
            ),
            row(
                "teamChannelRepos",
                "Repos for new work",
                "Which repo and Linear team new work from each Slack channel goes to.",
            ),
            row(
                "teamDefaultRunPlace",
                "Where new work runs",
                "Cloud or this computer, for @Ghostex in Slack without cloud or local.",
            ),
            row(
                "teamNeverWithoutTicket",
                "Never work without a ticket",
                "Ghostex finds the ticket in the thread or creates one in Linear before any session starts. Always on.",
            ),
            row(
                "teamInstructions",
                "Team instructions",
                "Your team's rules, added to every session Ghostex starts from Slack.",
            ),
            row(
                "teamFlowSteps",
                "Team-flow steps",
                "The steps each ticket shows on the Work page, shared by the whole team: reorder, rename, remove or add them, or reset to the default. Only the team's owners can change them.",
            ),
        ],
    )
}
