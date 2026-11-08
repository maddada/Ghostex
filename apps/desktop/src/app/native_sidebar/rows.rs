use super::drag::SidebarDropTarget;
use super::{appearance::SidebarAppearance, model::NativeSidebarGroup};
use crate::GhostexGpuiApp;
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div, px,
};
use gpui_component::v_flex;
use serde_json::{Value, json};

impl GhostexGpuiApp {
    /// CDXC:Bots 2026-09-27 DECISION:
    /// User: New Session under a bot starts a session with that bot, the same launch as its "+", so it opens in chat or terminal per the agent-interface setting instead of a plain terminal.
    ///
    /// The "New Session" row of a group with no sessions: a project's opens a terminal.
    fn start_native_sidebar_empty_group_session(
        &mut self,
        group_id: &str,
        is_bot: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if is_bot {
            self.dispatch_native_sidebar_ui(
                json!({"type": "projectAction", "action": "bot", "groupId": group_id}),
                cx,
            );
        } else {
            self.dispatch_native_sidebar_command(
                json!({"type": "createProjectTerminal", "groupId": group_id}),
                cx,
            );
        }
    }

    /// A project's block: its header and, while it is open or folding, its rows and then its
    /// `session_groups` (session_group_tree.rs), which fold with it.
    pub(crate) fn render_native_sidebar_group(
        &self,
        group: &NativeSidebarGroup,
        session_groups: &[&NativeSidebarGroup],
        hud: &Value,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let view = cx.entity().clone();
        let group_id = group.group_id.clone();
        let key = format!("group:{}", group.group_id);
        let scale = appearance.scale;
        // CDXC:Sidebar 2026-09-24 DECISION:
        // User: don't show the Sessions label under a project or worktree when it's the only label there (no pinned, drafts or parked sessions). Superseded 2026-10-04: it no longer comes back while a session is dragged, because a drag must never change the sidebar's layout.
        let hide_lone_sessions_heading =
            matches!(group.sections.as_slice(), [only] if only.id == "sessions");
        let body = self
            .native_sidebar
            .disclosures
            .present(&key, group.collapsed)
            .then(|| {
                let content = v_flex()
                    .w_full()
                    .pb(px(3.0 * scale))
                    // Main Hub before Chief of Staff's first open: its row's place (chief.rs).
                    .children(self.render_native_sidebar_chief_placeholder(group, appearance, cx))
                    .when(
                        group.sessions.is_empty() && group.project_context.is_some(),
                        |column| {
                            let id = group.group_id.clone();
                            let is_bot = group.bot_status().is_some();
                            let label = "New Session";
                            column.child(
                                div()
                                    .id(format!("native-empty-project-{id}"))
                                    .h(px(34.0 * scale))
                                    .mx(px(3.0 * scale))
                                    .pl(px(26.0 * scale))
                                    .flex()
                                    .items_center()
                                    .rounded(px(5.0 * scale))
                                    .hover(|row| row.bg(appearance.hover))
                                    .relative()
                                    .child(label)
                                    .sidebar_drop_target("session-group", id.clone(), None, cx)
                                    .on_click(cx.listener(move |app, _, _, cx| {
                                        cx.stop_propagation();
                                        app.start_native_sidebar_empty_group_session(
                                            &id, is_bot, cx,
                                        );
                                    })),
                            )
                        },
                    )
                    .children(group.sections.iter().enumerate().map(|(index, section)| {
                        let key = format!("section:{}:{}", group.group_id, section.id);
                        let ids = self.native_sidebar.disclosures.section_ids(
                            &key,
                            &section.session_ids,
                            section.collapsed,
                        );
                        let body = self
                            .native_sidebar
                            .disclosures
                            .present(&key, section.collapsed)
                            .then(|| {
                                let sessions = group
                                    .sessions
                                    .iter()
                                    .map(|session| (session.session_id.as_str(), session))
                                    .collect::<std::collections::HashMap<_, _>>();
                                let rows = self.render_native_session_rows(
                                    group,
                                    ids.iter()
                                        .filter_map(|id| {
                                            sessions
                                                .get(id.as_str())
                                                .map(|session| (*session).clone())
                                        })
                                        .collect(),
                                    appearance,
                                    cx,
                                );
                                self.render_native_disclosure(key, rows, cx)
                            });
                        v_flex()
                            .w_full()
                            .flex_shrink_0()
                            .when(index > 0 && group.sections[index - 1].collapsed, |column| {
                                column.mt(px(8.0 * scale))
                            })
                            .when(!hide_lone_sessions_heading, |column| {
                                column.child(
                                    self.render_native_section_header(
                                        group, section, appearance, cx,
                                    ),
                                )
                            })
                            .children(body)
                            .into_any_element()
                    }));
                self.render_native_project_fold(
                    key,
                    content.into_any_element(),
                    session_groups,
                    hud,
                    appearance,
                    cx,
                )
            });
        // The line of a project drop is drawn on the project's whole block, header and sessions,
        // because a project dropped after an expanded one lands after its sessions too.
        let drop_line = self.native_sidebar_project_drop_line("group", &group.group_id);
        let block = v_flex()
            .when(
                self.native_sidebar.is_dragging("group", &group.group_id),
                |row| row.opacity(0.18),
            )
            .on_children_prepainted(move |bounds, _, cx| {
                if let (Some(first), Some(last)) = (bounds.first(), bounds.last()) {
                    let bounds = gpui::Bounds {
                        origin: first.origin,
                        size: gpui::size(first.size.width, last.bottom() - first.top()),
                    };
                    view.update(cx, |app, cx| {
                        app.record_native_project_bounds(&group_id, bounds, cx)
                    });
                }
            })
            .w_full()
            .flex_shrink_0()
            .child(self.render_native_project_header(group, hud, appearance, cx))
            .children(body);
        div()
            .relative()
            .w_full()
            .flex_shrink_0()
            .child(block)
            .when_some(drop_line, |row, position| {
                // The project header's box reaches left over its chevron slot, so the line spans the box.
                row.child(super::drag::drop_line_spanning(
                    position,
                    scale,
                    px(-super::project_header::PROJECT_HEADER_CHEVRON_INSET * scale),
                    px(3.0 * scale),
                ))
            })
            .into_any_element()
    }
}
