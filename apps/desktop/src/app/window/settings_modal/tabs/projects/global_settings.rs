//! The sections inside the Projects page's Global settings fold (regions.rs): New projects, Files
//! (the Docs folders list) and Global Defaults, the values every project shares.
use super::super::super::fields::{FieldStates, settings_section, settings_textarea};
use super::super::super::palette::SettingsPalette;
use super::super::super::search::{TabSearch, should_show_section};
use super::super::super::store::SettingsValues;
use super::{ProjectsTab, plain_row, ticket_key};
use gpui::{AnyElement, Context, IntoElement as _, SharedString, Window};

impl ProjectsTab {
    /// The fold's sections a search leaves on screen (all of them while not searching).
    pub(super) fn global_sections(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        search: &TabSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let p = *p;
        let global_command = values.string("globalWorktreeCommand");
        let global_key = values.string("globalBeadsDisplayKey");
        let global_beads = values.string("globalBeadsDirectory");
        let global_docs = values.string("globalDocsDirectory");
        let mut sections: Vec<AnyElement> = Vec::new();

        // SEE-ALSO: apps/desktop/src/app/remote_conn/clone_job_and_preview.rs hands this folder to the Add Project dialog as This machine's start folder.
        if should_show_section(&search.section("newProjects"), None) {
            let input = self.setting_input(
                &p,
                "defaultProjectsFolder",
                &values.string("defaultProjectsFolder"),
                "~/dev",
                None,
                None,
                window,
                cx,
            );
            let row = plain_row(
                &p,
                &SharedString::from("default-projects-folder"),
                "Default projects folder",
                Some(
                    "Add Project's New project folder makes new projects here, and Local folder starts here. Left blank, Add Project suggests the folder that holds the selected project (else ~/dev) and saves the folder your first new project is made in.",
                ),
                None,
                true,
                input,
                cx,
            );
            if let Some(section) = settings_section(&p, "New projects", None, None, vec![row]) {
                sections.push(section.into_any_element());
            }
        }

        // CDXC:Docs 2026-06-30-11:42 SEE-ALSO: the Docs folders list is a global Projects setting above the selector, comma-separated project-relative folders, in a card titled after the view it controls.
        if should_show_section(&search.section("docs"), None) {
            let input = self.setting_input(
                &p,
                "manageAdditionalDocsFolders",
                &values.string("manageAdditionalDocsFolders"),
                "plans, my documents, folders/folder name",
                None,
                None,
                window,
                cx,
            );
            let row = plain_row(
                &p,
                &SharedString::from("docs-folders"),
                "Docs folders",
                Some(
                    "Comma-separated project-relative folders to scan recursively in the Files view. Spaces around folder names are ignored. Leave blank to scan docs/, artifacts/, ai/, and tmp/ at the project root and one folder down, plus root Markdown, HTML, and Excalidraw files. A Docs directory set below adds its whole tree on top of this.",
                ),
                None,
                true,
                input,
                cx,
            );
            if let Some(section) = settings_section(
                &p,
                "Files",
                Some("The Files view scans docs, artifacts, ai, and tmp by default, including those folders one level down. Add more project-relative folders here.".into()),
                None,
                vec![row],
            ) {
                sections.push(section.into_any_element());
            }
        }

        // CDXC:Projects 2026-08-02 SEE-ALSO: Global Defaults configure every project at once; a project's own non-empty value always wins.
        if should_show_section(&search.section("globalDefaults"), None) {
            let command_state = FieldStates::textarea_state(
                self,
                &SharedString::from("globalWorktreeCommand"),
                &global_command,
                Some("bun install"),
                (1, 20),
                |tab: &mut ProjectsTab, text, _window, cx| {
                    tab.save_setting("globalWorktreeCommand", text, cx)
                },
                window,
                cx,
            );
            let rows = vec![
                plain_row(
                    &p,
                    &SharedString::from("global-worktree-command"),
                    "Worktree command",
                    Some(
                        "Runs in every new worktree folder unless the project sets its own command.",
                    ),
                    None,
                    true,
                    settings_textarea(&p, &command_state, 96.0, false, false, window, cx),
                    cx,
                ),
                {
                    let input = self.setting_input(
                        &p,
                        "globalBeadsDisplayKey",
                        &global_key,
                        "ZMX",
                        Some(super::super::super::fields::CONTROL_LANE_WIDTH),
                        Some(ticket_key),
                        window,
                        cx,
                    );
                    plain_row(
                        &p,
                        &SharedString::from("global-ticket-key"),
                        "Ticket key",
                        Some(
                            "Ticket prefix for every project board unless the project sets its own key.",
                        ),
                        None,
                        false,
                        input,
                        cx,
                    )
                },
                {
                    let input = self.setting_input(
                        &p,
                        "globalBeadsDirectory",
                        &global_beads,
                        "/Users/you/code/my-repo",
                        None,
                        None,
                        window,
                        cx,
                    );
                    plain_row(
                        &p,
                        &SharedString::from("global-beads-directory"),
                        "Beads directory",
                        Some(
                            "Absolute path every Project board reads its Beads workspace (.beads) from unless the project sets its own directory. Leave blank to keep using each project root.",
                        ),
                        None,
                        true,
                        input,
                        cx,
                    )
                },
                {
                    let input = self.setting_input(
                        &p,
                        "globalDocsDirectory",
                        &global_docs,
                        "/Users/you/Documents/vault",
                        None,
                        None,
                        window,
                        cx,
                    );
                    // CDXC:Docs 2026-08-09 SEE-ALSO: the Docs directory is added beside each project's own docs, never swapped in for them.
                    plain_row(
                        &p,
                        &SharedString::from("global-docs-directory"),
                        "Docs directory",
                        Some(
                            "Extra folder every project's Files view shows unless the project sets its own. It is added alongside that project's own README, CLAUDE.md, docs/ and Docs folders; it never replaces them. It appears as one top-level folder named after itself. Leave blank to add nothing.",
                        ),
                        None,
                        true,
                        input,
                        cx,
                    )
                },
            ];
            if let Some(section) = settings_section(
                &p,
                "Global Defaults",
                Some("Applied to every project that does not set its own value below.".into()),
                None,
                rows,
            ) {
                sections.push(section.into_any_element());
            }
        }

        sections
    }
}
