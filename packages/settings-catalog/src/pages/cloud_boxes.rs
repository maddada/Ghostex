use crate::data::*;
use crate::json::{num_opt, opt, NumOpt};
use crate::rows::{row, section, Page, Section};

/// CDXC:AgentBox 2026-10-01 SEE-ALSO:
/// Settings > Cloud Boxes (apps/desktop/src/app/window/settings_modal/tabs/cloud_boxes.rs and
/// tabs/cloud-boxes.tsx (deleted 2026-10-01)). `agentboxDefaultLocation` is the agentbox part's one setting; the other rows are
/// keyed by UI ids so the search and Ghostex Help can find every part of the page.
///
/// CDXC:CloudBoxes 2026-11-02 DECISION: the user's Cloud Boxes picks (Q995 to Q1024, D1 to D13) put Instabox and Boat first on this page: Connect in the browser with a pasted key as the fallback (Q998), this month's spend and the monthly cap (Q1000, Q1001, D8), the 15-minute idle stop and the 7-day delete of archived boxes (Q1011, Q1012), every box with its actions (Q1017), agent logins for boxes (D5, D6), starting boxes from the phone (Q1016, D7); today's agentbox providers move under Advanced as "More providers" (Q997 "advanced").
pub(crate) fn page() -> Page {
    Page {
        id: "cloudBoxes",
        title: "Cloud Boxes",
        sections: vec![
            cloud_providers(),
            cloud_spending(),
            cloud_lifetime(),
            cloud_box_list(),
            cloud_agent_logins(),
            overview(),
            providers(),
            agent_sign_in(),
            new_threads(),
            boxes(),
            how_to(),
        ],
    }
}

/// Minutes with nothing running before a box pauses or stops (Q1011).
pub const CLOUD_BOXES_IDLE_STOP_MINUTE_OPTIONS: &[NumOpt] = &[
    num_opt("5 minutes", 5.0),
    num_opt("10 minutes", 10.0),
    num_opt("15 minutes", 15.0),
    num_opt("30 minutes", 30.0),
    num_opt("1 hour", 60.0),
    num_opt("2 hours", 120.0),
];

/// Days a stopped box of archived sessions is kept (Q1012).
pub const CLOUD_BOXES_DELETE_AFTER_DAY_OPTIONS: &[NumOpt] = &[
    num_opt("1 day", 1.0),
    num_opt("3 days", 3.0),
    num_opt("7 days", 7.0),
    num_opt("14 days", 14.0),
    num_opt("30 days", 30.0),
];

pub(crate) fn cloud_providers() -> Section {
    section(
        "cloudProviders",
        "Providers",
        vec![
            row("cloudBoxesInstabox", "Instabox", "Connect your Instabox.sh account in the browser, or paste an API key. Boxes run on your account and bill to it."),
            row("cloudBoxesBoat", "Boat", "Connect your Boat.dev account: Ghostex asks for your email, opens Boat's page and shows a 6-digit code to type there. Or paste an API key."),
        ],
    )
}

pub(crate) fn cloud_spending() -> Section {
    section(
        "cloudSpending",
        "Spending",
        vec![
            row("cloudBoxesSpend", "This month", "What your Cloud Boxes cost this month, per provider and in total. Instabox counts everything on that account; Boat counts the boxes Ghostex made."),
            row("cloudBoxesMonthlyCapUsd", "Monthly cap", "Ghostex warns at 80% of this many dollars a month and starts no new box at 100%. Running boxes keep running. 0 turns the cap off."),
        ],
    )
}

pub(crate) fn cloud_lifetime() -> Section {
    section(
        "cloudLifetime",
        "Stopping and deleting",
        vec![
            row("cloudBoxesIdleStopMinutes", "Stop idle boxes after", "A box with no agent working and no terminal open pauses (Instabox) or stops (Boat) after this long. Sending a message wakes it.").num_options(CLOUD_BOXES_IDLE_STOP_MINUTE_OPTIONS),
            row("cloudBoxesDeleteStoppedAfterDays", "Delete archived boxes after", "When every session on a box is archived, the box stops and is deleted after this many days. A box with unpushed work is kept until you push or delete it.").num_options(CLOUD_BOXES_DELETE_AFTER_DAY_OPTIONS),
            row("cloudBoxesPhoneStartEnabled", "Start boxes from your phone", "Keep your provider key, agent logins, Default snapshots and picked secrets on your paired phones, so they can start a box while this computer is off."),
        ],
    )
}

pub(crate) fn cloud_box_list() -> Section {
    section(
        "cloudBoxList",
        "Your boxes",
        vec![row("cloudBoxesBoxes", "Your boxes", "Every box with its state, project, age and cost. Stop a box, or delete it; a box with unpushed work asks first.")],
    )
}

pub(crate) fn cloud_agent_logins() -> Section {
    section(
        "cloudAgentLogins",
        "Agent logins for boxes",
        vec![
            row("cloudBoxesClaudeLogins", "Claude on cloud boxes", "Allow a Claude account on boxes: it makes a one-year box token after one approval in your browser, so Claude on this computer stays signed in."),
            row("cloudBoxesCodexLogins", "Codex on cloud boxes", "Allow a Codex account on boxes: each box gets a fresh copy of its login and sends refreshed sign-ins back to this computer."),
        ],
    )
}

pub(crate) fn overview() -> Section {
    section(
        "overview",
        "More providers (agentbox)",
        vec![
            row("agentboxStatus", "agentbox", "Install agentbox, the free open-source command line tool Ghostex uses to run agent sessions in boxes, see its version and whether Docker is running, and run its check."),
            row("agentboxSetUpForMe", "Set it up for me", "Start an agent that installs agentbox, asks which clouds you want, creates the API tokens in your browser, and signs Claude and Codex in for boxes."),
            row("agentboxWhatIsABox", "What is a box?", "A box is an isolated copy of your project where an agent works without touching this computer. Your agent's settings, skills and Codex sign-in go with it, and its web app opens on this computer."),
        ],
    )
}

pub(crate) fn providers() -> Section {
    section(
        "providers",
        "Where boxes run",
        vec![
            row("agentboxProviders", "Where boxes run", "Set up Docker on this computer, or log in to a cloud provider with an API token and prepare its base image once. Cloud boxes bill while they exist.").options(&[opt("Docker on this computer", "docker"), opt("Hetzner", "hetzner"), opt("Vercel", "vercel"), opt("Daytona", "daytona"), opt("E2B", "e2b"), opt("DigitalOcean", "digitalocean")]),
            row("agentboxRemoteDocker", "Your own server (SSH)", "Add your own server with Docker by a name and its SSH address (user@host or a name from ~/.ssh/config), then check that boxes can run there."),
        ],
    )
}

pub(crate) fn agent_sign_in() -> Section {
    section(
        "agentSignIn",
        "Agent sign-in",
        vec![
            row("agentboxClaudeSignIn", "Claude in boxes", "Claude needs its own one-time sign-in for boxes, so Claude on this computer stays signed in. Every box uses it."),
            row("agentboxCodexSignIn", "Codex in boxes", "Boxes on this computer reuse your Codex sign-in. Sign in here for cloud boxes when Codex is not signed in."),
        ],
    )
}

pub(crate) fn new_threads() -> Section {
    section(
        "newThreads",
        "New threads",
        vec![row(
            "agentboxDefaultLocation",
            "Default location",
            "Where new threads run unless you pick another location.",
        )
        .options(AGENTBOX_DEFAULT_LOCATION_OPTIONS)],
    )
}

pub(crate) fn boxes() -> Section {
    section(
        "boxes",
        "Your agentbox boxes",
        vec![row(
            "agentboxBoxes",
            "Your agentbox boxes",
            "See every box, open a box's web app, stop a box, or destroy it to delete it for good.",
        )],
    )
}

pub(crate) fn how_to() -> Section {
    section(
        "howTo",
        "How it works",
        vec![
            row("agentboxStartThread", "Start a thread in an agentbox box", "Open New Thread and pick where it runs under Run on. Boxes open in the terminal view."),
            row("agentboxWebApp", "Open a box's web app", "Right-click the session and choose Open Box Web App to open it on this computer. Add an agentbox.yaml with services.web.expose.port to start your dev server in the box automatically."),
            row("agentboxBilling", "Cloud boxes bill until stopped", "Cloud providers charge while a box exists. Deleting a session stops its box and keeps its work; Destroy removes the box."),
        ],
    )
}
