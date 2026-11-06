use crate::rows::{row, section, Page, Section};

pub(crate) fn page() -> Page {
    Page {
        id: "projects",
        title: "Projects",
        sections: vec![
            new_projects(),
            docs(),
            global_defaults(),
            project_settings(),
        ],
    }
}

pub(crate) fn new_projects() -> Section {
    section(
        "newProjects",
        "New projects",
        vec![row(
            "defaultProjectsFolder",
            "Default projects folder",
            "Folder that holds your projects, like ~/Projects. Add Project opens there and offers New project folder, which makes a subfolder and adds it as its own project.",
        )],
    )
}

pub(crate) fn docs() -> Section {
    section(
        "docs",
        "Files",
        vec![row(
            "docsFolders",
            "Docs folders",
            "Comma-separated project-relative folders to scan recursively in the Files view.",
        )],
    )
}

pub(crate) fn global_defaults() -> Section {
    section(
        "globalDefaults",
        "Global Defaults",
        vec![
            row("globalWorktreeCommand", "Global worktree command", "Worktree command every project uses unless it sets its own."),
            row("globalTicketKey", "Global ticket key", "Ticket key every project uses unless it sets its own."),
            row("globalBeadsDirectory", "Global Beads directory", "Beads directory every project uses unless it sets its own."),
            row("globalDocsDirectory", "Global Docs directory", "Extra folder the Files view shows in every project, alongside that project's own docs."),
        ],
    )
}

pub(crate) fn project_settings() -> Section {
    section(
        "projectSettings",
        "Project settings",
        vec![
            row("worktreeCommand", "Worktree command", "Runs in the new worktree folder before the project is added (useful for .envs, installing dependencies, etc.)."),
            row("ticketKey", "Ticket key", "Three-letter prefix used for Linear-style ticket numbers on the Project board."),
            row("beadsDirectory", "Beads directory", "Absolute path the Project board reads its Beads workspace (.beads) from."),
            row("docsDirectory", "Docs directory", "Extra folder this project's Files view shows, in addition to its own docs."),
            row("projectLinearApiKey", "Project Linear API key", "A Linear key for this project only, used instead of its workspace's or the shared key. Issues Workflow shows Linear tickets once a key applies to the project."),
            row("projectCloudBoxDefaults", "Cloud box provider and size", "Which provider and machine size a new Cloud Box for this project starts on."),
            row("projectCloudBoxSnapshots", "Cloud box snapshots", "This project's saved snapshots: what a new Cloud Box starts from. Make one the Default (★), update the Default from a running box, or delete one."),
            row("projectCloudBoxSecrets", "Send to the box", "Tick the project's .env files and name the variables a Cloud Box gets at start. Their values are read from this computer each time and never saved in a snapshot."),
        ],
    )
}
