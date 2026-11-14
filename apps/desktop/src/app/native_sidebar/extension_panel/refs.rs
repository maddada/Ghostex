//! A `{project}` or `{session}` item: Ghostex fills in the name and the live status dot from its
//! own store, hover text in the sidebar's bubble, the row's own right-click menu (the project's or
//! the session's, as the Projects list draws it), and a click opens it.

use ghostex_gx_core::{MachineId, SessionKey};
use ghostex_gx_protocol::extension_panel::{PanelAction, PanelRow};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use serde_json::Value;

use super::PanelPlace;
use super::icons::panel_icon;
use super::surface::{row_frame, with_hover_text};
use crate::GhostexGpuiApp;
use crate::app::native_sidebar::appearance::SidebarAppearance;
use crate::app::native_sidebar::status;

/// Which Ghostex menu an item's right-click opens.
#[derive(Clone, Debug)]
enum RefMenu {
    Project(String),
    Session(String),
    None,
}

/// What the store knows about a project a panel names.
struct ProjectFacts {
    project_id: String,
    title: String,
    path: Option<String>,
    sessions: usize,
    working: usize,
    attention: usize,
    questions: usize,
    approvals: usize,
}

/// What the store knows about a session a panel names.
struct SessionFacts {
    sidebar_session_id: String,
    title: String,
    activity: String,
    question: bool,
    approval: bool,
}

impl GhostexGpuiApp {
    fn native_panel_project_facts(&self, reference: &str) -> Option<ProjectFacts> {
        let loaded = self
            .gx_store
            .core
            .presentation()
            .loaded_live(&MachineId::Local)?;
        let project_id = ghostex_gx_core::extension_panel::resolve_panel_project(
            reference,
            loaded
                .projects()
                .iter()
                .map(|project| (project.project_id.as_str(), project.path.as_deref())),
            std::env::var("HOME").ok().as_deref(),
        )?;
        let project = loaded
            .projects()
            .iter()
            .find(|project| project.project_id == project_id)?;
        let mut facts = ProjectFacts {
            project_id: project_id.clone(),
            title: project.title.clone(),
            path: project.path.clone(),
            sessions: 0,
            working: 0,
            attention: 0,
            questions: 0,
            approvals: 0,
        };
        for session in loaded
            .server_sessions()
            .filter(|session| session.project_id == project_id)
            .filter(|session| session.listed_in_sidebar())
        {
            facts.sessions += 1;
            match session.activity.as_str() {
                "working" => facts.working += 1,
                "attention" => facts.attention += 1,
                _ => {}
            }
            facts.questions +=
                usize::from(session.awaiting_answer || session.pending_question_count > 0);
            facts.approvals += usize::from(session.awaiting_approval);
        }
        Some(facts)
    }

    fn native_panel_session_facts(&self, session_ref: &str) -> Option<SessionFacts> {
        let (project_id, session_id) = session_ref.split_once(':')?;
        let loaded = self
            .gx_store
            .core
            .presentation()
            .loaded_live(&MachineId::Local)?;
        let session = loaded
            .server_sessions()
            .find(|session| session.project_id == project_id && session.session_id == session_id)?;
        let title = [&session.primary_title, &session.display_title]
            .into_iter()
            .flatten()
            .find(|title| !title.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| session.title.clone());
        Some(SessionFacts {
            sidebar_session_id: SessionKey {
                machine: MachineId::Local,
                project_id: project_id.to_string(),
                session_id: session_id.to_string(),
            }
            .to_sidebar_session_id(),
            title,
            activity: session.activity.as_str().to_string(),
            question: session.awaiting_answer || session.pending_question_count > 0,
            approval: session.awaiting_approval,
        })
    }

    /// A `project` or `session` row of a rows or map block.
    pub(super) fn render_native_panel_ref_row(
        &self,
        row: &PanelRow,
        element_id: String,
        place: &PanelPlace,
        block_id: &str,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let extension_id = place.extension_id.clone();
        let (title, dot, hover, open, menu) = if let Some(reference) = &row.project {
            match self.native_panel_project_facts(reference) {
                Some(facts) => {
                    let dot = if facts.approvals > 0 {
                        Some(status::APPROVAL_COLOR)
                    } else if facts.questions > 0 {
                        Some(status::QUESTION_COLOR)
                    } else if facts.attention > 0 {
                        Some(0x95d7f6)
                    } else if facts.working > 0 {
                        Some(status::WORKING_COLOR)
                    } else {
                        None
                    };
                    // The project header's own tooltip; this summary only while the list has
                    // never been built.
                    let hover = self
                        .gx_store_panel_ref_hover(Some(&facts.project_id), None)
                        .unwrap_or_else(|| {
                            let mut hover = facts.title.clone();
                            if let Some(path) = &facts.path {
                                hover.push('\n');
                                hover.push_str(path);
                            }
                            hover.push_str(&format!("\n{} sessions", facts.sessions));
                            hover
                        });
                    (
                        row.title.clone().unwrap_or(facts.title),
                        dot,
                        hover,
                        Some(PanelAction::OpenProject(facts.project_id.clone())),
                        RefMenu::Project(facts.project_id.clone()),
                    )
                }
                None => (
                    row.title.clone().unwrap_or_else(|| reference.clone()),
                    None,
                    format!("{reference} isn't a project on this computer."),
                    None,
                    RefMenu::None,
                ),
            }
        } else {
            let reference = row.session.clone().unwrap_or_default();
            match self.native_panel_session_facts(&reference) {
                Some(facts) => {
                    let dot = if facts.approval {
                        Some(status::APPROVAL_COLOR)
                    } else if facts.question {
                        Some(status::QUESTION_COLOR)
                    } else {
                        match facts.activity.as_str() {
                            "working" => Some(status::WORKING_COLOR),
                            "attention" => Some(0x95d7f6),
                            _ => None,
                        }
                    };
                    (
                        row.title.clone().unwrap_or_else(|| facts.title.clone()),
                        dot,
                        self.gx_store_panel_ref_hover(None, Some(&facts.sidebar_session_id))
                            .unwrap_or_else(|| format!("{}\n{}", facts.title, facts.activity)),
                        Some(PanelAction::FocusSession(reference.clone())),
                        RefMenu::Session(facts.sidebar_session_id.clone()),
                    )
                }
                None => (
                    row.title
                        .clone()
                        .unwrap_or_else(|| "Session not found".to_string()),
                    None,
                    "This session isn't on this computer anymore.".to_string(),
                    None,
                    RefMenu::None,
                ),
            }
        };
        // A row's own action wins over opening what it names.
        let action = row.action.clone().or(open);
        let missing = action.is_none();
        let click_place = place.clone();
        let click_block = block_id.to_string();
        let click_item = row.id.clone();
        let frame = row_frame(element_id, appearance)
            .child(
                div()
                    .w(px(status::STATUS_SLOT_WIDTH * scale))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(match dot {
                        Some(color) => div()
                            .size(px(status::STATUS_DOT_SIZE * scale))
                            .rounded_full()
                            .bg(rgb(color))
                            .into_any_element(),
                        None => panel_icon(
                            Some(row.icon.as_deref().unwrap_or(if row.project.is_some() {
                                "folder"
                            } else {
                                "message"
                            })),
                            &extension_id,
                            14.0 * scale,
                            appearance.chrome_icon,
                        )
                        .unwrap_or_else(|| div().into_any_element()),
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .when(missing, |title| title.text_color(appearance.muted))
                    .child(title),
            )
            .when_some(row.trailing.clone(), |frame, trailing| {
                frame.child(
                    div()
                        .flex_shrink_0()
                        .text_color(appearance.muted)
                        .child(trailing),
                )
            })
            .when_some(action, |frame, action| {
                frame.on_click(cx.listener(move |app, _, window, cx| {
                    cx.stop_propagation();
                    app.run_native_panel_action(
                        &click_place,
                        &click_block,
                        click_item.as_deref(),
                        &action,
                        window,
                        cx,
                    );
                }))
            })
            .when(
                !matches!(menu, RefMenu::None) || row.menu.is_some(),
                |frame| {
                    // The panel's own items first, then the row's Ghostex menu, built when it
                    // opens from the whole Projects list (gx_store/panel_refs.rs).
                    let own = row.menu.as_ref().map(|items| {
                        Self::native_panel_menu(
                            place,
                            block_id,
                            row.id.as_deref(),
                            Some(items),
                            false,
                        )
                    });
                    frame.on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |app, event: &gpui::MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            let ghostex = match &menu {
                                RefMenu::Project(project_id) => {
                                    app.gx_store_panel_project_menu(project_id)
                                }
                                RefMenu::Session(session_id) => {
                                    app.gx_store_panel_session_menu(session_id)
                                }
                                RefMenu::None => None,
                            };
                            let mut items: Vec<Value> = own
                                .as_ref()
                                .and_then(Value::as_array)
                                .cloned()
                                .unwrap_or_default();
                            if let Some(Value::Array(ghostex)) = ghostex {
                                if !items.is_empty() && !ghostex.is_empty() {
                                    items.push(serde_json::json!({"separator": true}));
                                }
                                items.extend(ghostex);
                            }
                            if items.is_empty() {
                                return;
                            }
                            Self::show_native_sidebar_context_menu(
                                &Value::Array(items),
                                None,
                                event.position,
                                scale,
                                window,
                                cx,
                            );
                        }),
                    )
                },
            );
        with_hover_text(frame, Some(hover), appearance).into_any_element()
    }
}
