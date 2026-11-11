//! The Projects page under its title, as two hairline panels: Global settings, a fold whose row is
//! its panel's header and which holds the values every project shares (global_settings.rs), then
//! the project's panel, headed by the Project dropdown (picker.rs) and holding the picked
//! project's sections.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{FoldFrame, FoldRow, settings_fold_row};
use super::super::super::page::PageBlock;
use super::super::super::palette::SettingsPalette;
use super::super::super::search::{TabSearch, should_show_section};
use super::super::super::store::SettingsValues;
use super::{ProjectItem, ProjectsTab, no_projects, picker};
use gpui::{
    AnyElement, Context, HighlightStyle, IntoElement, ParentElement as _, SharedString,
    Styled as _, StyledText, Window, div, px,
};
use gpui_component::v_flex;
use std::ops::Range;

/// The catalog sections the Global settings fold holds (packages/settings-catalog/src/pages/projects.rs).
const GLOBAL_SECTIONS: [&str; 3] = ["newProjects", "docs", "globalDefaults"];

/// The catalog sections of the picked project's panel (the custom views have none).
const PROJECT_SECTIONS: [&str; 3] = ["projectSettings", "issuesWorkflow", "projectCloudBox"];

/// The fold row's text while it is open, and while nothing in it is set.
const GLOBAL_NOTE: &str = "Shared by every project. A project's own value below wins.";

impl ProjectsTab {
    /// The Global settings panel, then the project's panel (or "No projects").
    ///
    /// CDXC:Projects 2026-11-10 DECISION:
    /// The owner: "best would be to have just a page called projects, then have 2 dropdowns, 1 to select the project … the 1st dropdown is for sure the way to switch projects IMO, not having all in sidebar". User: the global settings come first, in a fold, enclosed so they read apart from the project's own settings. So the rail keeps one Projects row, and the page holds two hairline panels: Global settings (New projects, Files, Global Defaults), a fold that starts closed with a one-line summary of the values that are set and that a search matching one of its rows opens; then the project, headed by the Project dropdown (no "All projects" entry; it starts on the project of the session open in the main window) and holding Project settings, Issues Workflow (the Linear API key), Cloud box and one section per custom view. The owner's second dropdown, a machine picker, is left for later.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn page_regions(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        search: &TabSearch,
        projects: &[ProjectItem],
        selected: Option<ProjectItem>,
        globals: [&str; 4],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<PageBlock> {
        let mut blocks = Vec::new();
        let global_matches = GLOBAL_SECTIONS
            .iter()
            .any(|id| should_show_section(&search.section(id), None));
        if global_matches {
            // While searching, the fold is open for the rows it matched; clearing the search puts
            // it back the way it was.
            let searching = search.tab.is_searching;
            let open = searching || self.global_open;
            let header = settings_fold_row(
                p,
                FoldRow {
                    id: "settings-projects-global-fold",
                    label: "Global settings",
                    open,
                    frame: FoldFrame::PanelHeader,
                    trailing: Some(fold_trailing(p, open, values)),
                    held_open: searching.then_some("Open while the search matches settings in it."),
                },
                |tab: &mut Self, _window, cx| {
                    tab.global_open = !tab.global_open;
                    cx.notify();
                },
                cx,
            );
            let sections = open.then(|| self.global_sections(p, values, search, window, cx));
            blocks.push(PageBlock::section(
                "globalSettings",
                region_panel(p, header, sections, 20.0),
            ));
        }
        let project_matches = PROJECT_SECTIONS
            .iter()
            .any(|id| should_show_section(&search.section(id), None));
        if project_matches {
            match selected {
                None => blocks.push(PageBlock::plain(no_projects(p))),
                Some(project) => {
                    let header = self.project_header(p, projects, &project, window, cx);
                    let custom_views = values.value("customViews");
                    let sections = self.project_sections(
                        p,
                        projects,
                        &project,
                        globals,
                        &custom_views,
                        search,
                        window,
                        cx,
                    );
                    blocks.push(PageBlock::section(
                        "projectSettings",
                        region_panel(p, header, Some(sections), 24.0),
                    ));
                }
            }
        }
        blocks
    }

    /// The project panel's header: "Project" over the dropdown.
    fn project_header(
        &mut self,
        p: &SettingsPalette,
        projects: &[ProjectItem],
        project: &ProjectItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dropdown = picker::project_picker(self, p, projects, project, window, cx);
        v_flex()
            .w_full()
            .px(px(20.0))
            .py(px(14.0))
            .gap(px(6.0))
            .border_b_1()
            .border_color(hsla(p.hairline))
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(hsla(p.muted))
                    .child("Project"),
            )
            .child(dropdown)
            .into_any_element()
    }
}

/// A hairline panel: its header row, then `sections` (when it shows them) `top` px under it.
fn region_panel(
    p: &SettingsPalette,
    header: AnyElement,
    sections: Option<Vec<AnyElement>>,
    top: f32,
) -> AnyElement {
    v_flex()
        .w_full()
        .rounded(px(MODAL_RADIUS_SECTION))
        .border_1()
        .border_color(hsla(p.hairline))
        .bg(hsla(p.foreground_alpha(0.012)))
        .child(header)
        .children(sections.map(|sections| {
            v_flex()
                .w_full()
                .pt(px(top))
                .px(px(16.0))
                .pb(px(16.0))
                .gap(px(24.0))
                .children(sections)
        }))
        .into_any_element()
}

/// The fold row's right side: closed, the summary of the values that are set; open (or with
/// nothing set), what the fold holds.
fn fold_trailing(p: &SettingsPalette, open: bool, values: &SettingsValues) -> AnyElement {
    let pairs = global_summary(&[
        ("Projects folder", values.string("defaultProjectsFolder")),
        ("Docs folders", values.string("manageAdditionalDocsFolders")),
        ("Worktree command", values.string("globalWorktreeCommand")),
        ("Ticket key", values.string("globalBeadsDisplayKey")),
        ("Beads directory", values.string("globalBeadsDirectory")),
        ("Docs directory", values.string("globalDocsDirectory")),
    ]);
    let line = div()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_size(px(13.0))
        .line_height(px(17.5))
        .text_color(hsla(p.muted));
    if open || pairs.is_empty() {
        return line.child(GLOBAL_NOTE).into_any_element();
    }
    let (text, highlights) = summary_text(&pairs, css_fade(p.muted, 0.7));
    line.child(StyledText::new(SharedString::from(text)).with_highlights(highlights))
        .into_any_element()
}

/// The set values, labelled, in page order; a value of several lines shows its first line.
fn global_summary(fields: &[(&'static str, String)]) -> Vec<(&'static str, String)> {
    fields
        .iter()
        .filter_map(|(label, value)| {
            let mut lines = value.trim().lines().map(str::trim);
            let first = lines.next().filter(|line| !line.is_empty())?;
            let shown = if lines.any(|line| !line.is_empty()) {
                format!("{first} …")
            } else {
                first.to_string()
            };
            Some((*label, shown))
        })
        .collect()
}

/// "Label value · Label value", with the labels dimmer than the values.
fn summary_text(
    pairs: &[(&'static str, String)],
    label_color: gpui::Rgba,
) -> (String, Vec<(Range<usize>, HighlightStyle)>) {
    let mut text = String::new();
    let mut highlights = Vec::new();
    for (index, (label, value)) in pairs.iter().enumerate() {
        if index > 0 {
            text.push_str("  ·  ");
        }
        let start = text.len();
        text.push_str(label);
        highlights.push((
            start..text.len(),
            HighlightStyle {
                color: Some(hsla(label_color)),
                ..Default::default()
            },
        ));
        text.push(' ');
        text.push_str(value);
    }
    (text, highlights)
}
