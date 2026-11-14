//! Sidebar extensions' panels as the renderers draw them: which modes and sections the sidebar
//! shows, which blocks a tab shows, when a map becomes an indented tree, the small Markdown
//! subset of a text block, which project a `project` item names, and which built-in actions the
//! user granted. The panels themselves come from gxserver (`extensionPanels`, kept as this
//! computer's side state); the desktop draws them in `native_sidebar/extension_panel/`, and the
//! same rules serve its side panel view.
//!
//! SEE-ALSO: packages/gx-protocol/src/extension_panel.rs (the document), server/src/extensions/
//! panel.rs (gxserver's checks).

use std::collections::BTreeMap;

use ghostex_gx_protocol::extension_panel::{
    ExtensionPanelEntry, ExtensionPanels, PanelAction, PanelBlock, PanelBlockBody, PanelDocument,
    SidePanelScope, SidebarSectionPlace,
};

use super::inputs::SidebarMode;

/// Narrower than this (in points), a map is drawn as an indented tree instead of group cards.
pub const MAP_TREE_BELOW_WIDTH: f32 = 340.0;
/// A text block shows at most this many lines in the sidebar; the rest is behind "Show more".
pub const TEXT_PREVIEW_LINES: usize = 12;

/// The sidebar's part: one button per extension mode, the mode on screen, and the sections.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExtensionSidebarView {
    pub modes: Vec<ExtensionModeButton>,
    pub active: Option<ExtensionSurfaceView>,
    pub sections_top: Vec<ExtensionSectionView>,
    pub sections_bottom: Vec<ExtensionSectionView>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionModeButton {
    pub extension_id: String,
    pub title: String,
    /// The icon gxserver serves (`/ext/<id>/<icon>`).
    pub icon: Option<String>,
    pub selected: bool,
}

/// A whole document as one surface draws it: a sidebar mode or a side panel view.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtensionSurfaceView {
    pub extension_id: String,
    pub title: String,
    pub icon: Option<String>,
    /// `<projectId>:<sessionId>` for a per-session side panel.
    pub session: Option<String>,
    pub revision: u64,
    pub tabs: Vec<ExtensionTabView>,
    pub blocks: Vec<ExtensionBlockView>,
    pub granted_permissions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionTabView {
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
    pub selected: bool,
}

/// One block of a section in the Projects list.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtensionSectionView {
    pub extension_id: String,
    pub extension_title: String,
    pub revision: u64,
    pub block: ExtensionBlockView,
    pub granted_permissions: Vec<String>,
}

/// A block and how it is laid out where it is drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtensionBlockView {
    pub block: PanelBlock,
    /// For a map: an indented tree rather than group cards.
    pub map_as_tree: bool,
    /// For a text block: its lines, at most [`TEXT_PREVIEW_LINES`] unless `text_expanded`.
    pub text_lines: Vec<TextLine>,
    /// The text block has more lines than it shows.
    pub text_truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextLineKind {
    Paragraph,
    Bullet,
    Numbered(u32),
    /// An empty line between paragraphs.
    Gap,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextLine {
    pub kind: TextLineKind,
    pub spans: Vec<TextSpan>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextSpan {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub link: Option<String>,
}

/// What a surface remembers between frames: the tab each surface shows and the text blocks the
/// user expanded, by `<extension>|<surface>`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtensionPanelUiState {
    pub selected_tabs: BTreeMap<String, String>,
    pub expanded_texts: std::collections::BTreeSet<String>,
    /// Blocks whose fold the user flipped from how the panel starts it (`folded`), by
    /// `<extension>|<surface>|<block>`.
    pub flipped_folds: std::collections::BTreeSet<String>,
    /// `rows` and `checklist` blocks whose items past `show` the user opened, by the same key.
    pub expanded_lists: std::collections::BTreeSet<String>,
}

impl ExtensionPanelUiState {
    pub fn surface_key(extension_id: &str, surface: &str) -> String {
        format!("{extension_id}|{surface}")
    }
}

/// The sidebar's extension modes and sections, for the drawn mode and width.
pub fn extension_sidebar_view(
    panels: Option<&ExtensionPanels>,
    mode: &SidebarMode,
    ui: &ExtensionPanelUiState,
    width: f32,
) -> ExtensionSidebarView {
    let mut view = ExtensionSidebarView::default();
    let Some(panels) = panels else {
        return view;
    };
    let mut modes = Vec::new();
    for (id, entry) in &panels.0 {
        let Some(sidebar) = &entry.sidebar else {
            continue;
        };
        if let Some(mode_entry) = &sidebar.mode {
            let selected = mode.extension_id() == Some(id.as_str());
            modes.push(ExtensionModeButton {
                extension_id: id.clone(),
                title: mode_entry.title.clone(),
                icon: mode_entry.icon.clone().or_else(|| entry.icon.clone()),
                selected,
            });
            if selected {
                view.active = Some(surface_view(
                    id,
                    entry,
                    &mode_entry.title,
                    mode_entry.icon.clone().or_else(|| entry.icon.clone()),
                    None,
                    sidebar.revision,
                    &sidebar.document,
                    ui,
                    "sidebar",
                    width,
                ));
            }
        }
        // A mode on screen replaces the Projects list, sections and all.
        if mode.extension_id().is_some() {
            continue;
        }
        for section in &sidebar.sections {
            let Some(block) = sidebar
                .document
                .blocks
                .iter()
                .find(|block| block.id == section.block)
            else {
                continue;
            };
            let section_view = ExtensionSectionView {
                extension_id: id.clone(),
                extension_title: entry.title.clone(),
                revision: sidebar.revision,
                block: block_view(block, id, "sidebar", ui, width),
                granted_permissions: entry.granted_permissions.clone(),
            };
            match section.place {
                SidebarSectionPlace::ProjectsTop => view.sections_top.push(section_view),
                SidebarSectionPlace::ProjectsBottom => view.sections_bottom.push(section_view),
            }
        }
    }
    modes.sort_by(|left, right| left.title.cmp(&right.title));
    view.modes = modes;
    view
}

/// Whether the saved mode is an extension's that no longer offers one (turned off, uninstalled,
/// or its mode switched off in Settings): the sidebar then goes back to the projects.
pub fn extension_mode_is_gone(panels: Option<&ExtensionPanels>, mode: &SidebarMode) -> bool {
    let Some(id) = mode.extension_id() else {
        return false;
    };
    !panels
        .and_then(|panels| panels.0.get(id))
        .and_then(|entry| entry.sidebar.as_ref())
        .is_some_and(|sidebar| sidebar.mode.is_some())
}

/// The side panel views the extensions offer: `(extension id, title, icon)`, by title.
pub fn extension_side_panels(
    panels: Option<&ExtensionPanels>,
) -> Vec<(String, String, Option<String>)> {
    let mut side_panels = panels
        .map(|panels| {
            panels
                .0
                .iter()
                .filter_map(|(id, entry)| {
                    let side_panel = entry.side_panel.as_ref()?;
                    Some((
                        id.clone(),
                        side_panel.title.clone(),
                        side_panel.icon.clone().or_else(|| entry.icon.clone()),
                    ))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    side_panels.sort_by(|left, right| left.1.cmp(&right.1));
    side_panels
}

/// An extension's side panel view: its computer-wide document, or (`scope: session`) the one
/// written for `session`, else the template every session starts from.
pub fn extension_side_panel_view(
    panels: Option<&ExtensionPanels>,
    extension_id: &str,
    session: Option<&str>,
    ui: &ExtensionPanelUiState,
    width: f32,
) -> Option<ExtensionSurfaceView> {
    let entry = panels?.0.get(extension_id)?;
    let side_panel = entry.side_panel.as_ref()?;
    let icon = side_panel.icon.clone().or_else(|| entry.icon.clone());
    let (session, revision, document) = match side_panel.scope {
        SidePanelScope::Global => (None, side_panel.revision, &side_panel.document),
        SidePanelScope::Session => {
            let session = session?;
            match side_panel.sessions.get(session) {
                Some(written) => (
                    Some(session.to_string()),
                    written.revision,
                    &written.document,
                ),
                None => (Some(session.to_string()), 0, &side_panel.document),
            }
        }
    };
    Some(surface_view(
        extension_id,
        entry,
        &side_panel.title,
        icon,
        session,
        revision,
        document,
        ui,
        "sidePanel",
        width,
    ))
}

#[allow(clippy::too_many_arguments)]
fn surface_view(
    extension_id: &str,
    entry: &ExtensionPanelEntry,
    title: &str,
    icon: Option<String>,
    session: Option<String>,
    revision: u64,
    document: &PanelDocument,
    ui: &ExtensionPanelUiState,
    surface: &str,
    width: f32,
) -> ExtensionSurfaceView {
    let key = ExtensionPanelUiState::surface_key(extension_id, surface);
    let tabs = document.tabs.clone().unwrap_or_default();
    // The remembered tab while the panel still has it, else the first.
    let selected = ui
        .selected_tabs
        .get(&key)
        .filter(|tab| tabs.iter().any(|known| &&known.id == tab))
        .cloned()
        .or_else(|| tabs.first().map(|tab| tab.id.clone()));
    let blocks = document
        .blocks
        .iter()
        // A block without a tab shows under every tab.
        .filter(|block| match (&block.tab, &selected) {
            (Some(tab), Some(selected)) => tab == selected,
            _ => true,
        })
        .filter(|block| !matches!(block.body, PanelBlockBody::Unknown))
        .map(|block| block_view(block, extension_id, surface, ui, width))
        .collect();
    ExtensionSurfaceView {
        extension_id: extension_id.to_string(),
        title: document.title.clone().unwrap_or_else(|| title.to_string()),
        icon,
        session,
        revision,
        tabs: tabs
            .into_iter()
            .map(|tab| ExtensionTabView {
                selected: selected.as_deref() == Some(tab.id.as_str()),
                id: tab.id,
                title: tab.title,
                icon: tab.icon,
            })
            .collect(),
        blocks,
        granted_permissions: entry.granted_permissions.clone(),
    }
}

fn block_view(
    block: &PanelBlock,
    extension_id: &str,
    surface: &str,
    ui: &ExtensionPanelUiState,
    width: f32,
) -> ExtensionBlockView {
    let (text_lines, text_truncated) = match &block.body {
        PanelBlockBody::Text { text } => {
            let lines = parse_text(text);
            let expanded = ui.expanded_texts.contains(&format!(
                "{}|{}",
                ExtensionPanelUiState::surface_key(extension_id, surface),
                block.id
            ));
            if !expanded && lines.len() > TEXT_PREVIEW_LINES {
                (lines[..TEXT_PREVIEW_LINES].to_vec(), true)
            } else {
                (lines, false)
            }
        }
        _ => (Vec::new(), false),
    };
    ExtensionBlockView {
        block: block.clone(),
        map_as_tree: width < MAP_TREE_BELOW_WIDTH,
        text_lines,
        text_truncated,
    }
}

/// This computer's whole Projects list, whatever the sidebar shows right now: the Space, the tag
/// filters, hidden items and projects without sessions lifted. A panel's `project` and `session`
/// items read their hover text and right-click menu from it, so they don't depend on the Space on
/// screen (or on the extension mode, which draws no list at all).
pub fn unfiltered_local_view(
    core: &crate::Core,
    inputs: &super::inputs::SidebarInputs,
    now_ms: u64,
) -> super::view::SidebarView {
    let mut probe = super::reveal::unfiltered(inputs, SidebarMode::Projects);
    probe.ui.selected_machine_id = crate::LOCAL_MACHINE_ID.to_string();
    probe.ui.show_all_projects = true;
    super::model::SidebarViewModel::build_from_scratch(core, &probe, now_ms)
}

/// The hover text of every project and session row of `view`: by sidebar group id, and by
/// sidebar session id.
pub fn panel_ref_hover_texts(
    view: &super::view::SidebarView,
) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let mut projects = BTreeMap::new();
    let mut sessions = BTreeMap::new();
    for group in &view.groups {
        if let Some(tooltip) = &group.core.title_tooltip {
            projects.insert(group.core.group_id.clone(), tooltip.clone());
        }
        for session in &group.core.sessions {
            sessions.insert(
                session.row.sidebar_session_id.clone(),
                session.row.title_tooltip.clone(),
            );
        }
    }
    (projects, sessions)
}

/// Whether the user granted what `action` needs.
pub fn panel_action_allowed(action: &PanelAction, granted_permissions: &[String]) -> bool {
    action
        .required_permission()
        .is_none_or(|needed| granted_permissions.iter().any(|granted| granted == needed))
}

/// The project a `project` item or `openProject` action names: a project id, or a path
/// (`~/dev/app`, `/home/me/dev/app`), matched against `projects` (`(project id, path)`).
pub fn resolve_panel_project<'a>(
    reference: &str,
    projects: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
    home: Option<&str>,
) -> Option<String> {
    let reference = reference.trim();
    let wanted = normalize_path(reference, home);
    let mut by_path = None;
    for (project_id, path) in projects {
        if project_id == reference {
            return Some(project_id.to_string());
        }
        if by_path.is_none() && path.is_some_and(|path| normalize_path(path, home) == wanted) {
            by_path = Some(project_id.to_string());
        }
    }
    by_path
}

fn normalize_path(path: &str, home: Option<&str>) -> String {
    let expanded = match (path.strip_prefix('~'), home) {
        (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with('/') => {
            format!("{}{rest}", home.trim_end_matches('/'))
        }
        _ => path.to_string(),
    };
    let trimmed = expanded.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

/// A text block's Markdown subset: paragraphs, `-`/`*` bullets, `1.` numbered items, and
/// `**bold**`, `*italic*`/`_italic_`, `` `code` `` and `[label](url)` inside a line.
pub fn parse_text(text: &str) -> Vec<TextLine> {
    let mut lines: Vec<TextLine> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            if lines
                .last()
                .is_some_and(|last| last.kind != TextLineKind::Gap)
            {
                lines.push(TextLine {
                    kind: TextLineKind::Gap,
                    spans: Vec::new(),
                });
            }
            continue;
        }
        let (kind, body) =
            if let Some(rest) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
                (TextLineKind::Bullet, rest)
            } else if let Some((number, rest)) = line.split_once(". ").filter(|(number, _)| {
                !number.is_empty()
                    && number.len() <= 3
                    && number.bytes().all(|byte| byte.is_ascii_digit())
            }) {
                (TextLineKind::Numbered(number.parse().unwrap_or(1)), rest)
            } else {
                (TextLineKind::Paragraph, line)
            };
        lines.push(TextLine {
            kind,
            spans: parse_spans(body),
        });
    }
    while lines
        .last()
        .is_some_and(|last| last.kind == TextLineKind::Gap)
    {
        lines.pop();
    }
    lines
}

fn parse_spans(text: &str) -> Vec<TextSpan> {
    let mut spans: Vec<TextSpan> = Vec::new();
    let mut current = TextSpan::default();
    let (mut bold, mut italic) = (false, false);
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    let flush = |current: &mut TextSpan, spans: &mut Vec<TextSpan>, bold: bool, italic: bool| {
        if !current.text.is_empty() {
            spans.push(std::mem::take(current));
        }
        current.bold = bold;
        current.italic = italic;
    };
    while index < chars.len() {
        let rest: String = chars[index..].iter().collect();
        if rest.starts_with("**") {
            flush(&mut current, &mut spans, !bold, italic);
            bold = !bold;
            index += 2;
            continue;
        }
        // `snake_case` stays literal: `_` opens italics only at a word's start.
        let word_inside = index > 0 && chars[index - 1].is_alphanumeric();
        if (chars[index] == '*' || (chars[index] == '_' && (italic || !word_inside)))
            && (italic || rest[1..].contains(chars[index]))
        {
            flush(&mut current, &mut spans, bold, !italic);
            italic = !italic;
            index += 1;
            continue;
        }
        if chars[index] == '`' {
            if let Some(end) = rest[1..].find('`') {
                flush(&mut current, &mut spans, bold, italic);
                spans.push(TextSpan {
                    text: rest[1..1 + end].to_string(),
                    code: true,
                    ..TextSpan::default()
                });
                index += rest[..end + 2].chars().count();
                continue;
            }
        }
        if chars[index] == '[' {
            if let Some((label, url, used)) = parse_link(&rest) {
                flush(&mut current, &mut spans, bold, italic);
                spans.push(TextSpan {
                    text: label,
                    bold,
                    italic,
                    link: Some(url),
                    ..TextSpan::default()
                });
                index += used;
                continue;
            }
        }
        current.text.push(chars[index]);
        index += 1;
    }
    flush(&mut current, &mut spans, bold, italic);
    spans
}

/// `[label](url)` at the start of `rest`: the label, the url, and how many characters it used.
fn parse_link(rest: &str) -> Option<(String, String, usize)> {
    let close = rest.find("](")?;
    let end = rest[close + 2..].find(')')? + close + 2;
    let label = rest[1..close].to_string();
    let url = rest[close + 2..end].trim().to_string();
    let safe = url.starts_with("https://") || url.starts_with("http://");
    (!label.is_empty() && safe).then(|| (label, url, rest[..=end].chars().count()))
}
