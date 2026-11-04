use crate::GhostexGpuiApp;
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Bounds, Context, DragMoveEvent, InteractiveElement, IntoElement, ParentElement,
    Pixels, Point, Render, Styled, Window, div, px,
};
use serde_json::{Value, json};

#[derive(Clone)]
pub(crate) struct SidebarDrag {
    pub(crate) kind: &'static str,
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) scale: f32,
    pub(crate) preview: SidebarDragPreview,
    /// Set while the pointer is over a pane that refuses this drag
    /// (`session_pane_placement.rs`); shared by the drag's value and its preview, which fades.
    pub(crate) refused: std::rc::Rc<std::cell::Cell<bool>>,
}

/// How strongly the dragged row shows while it is over a pane that cannot take it.
const REFUSED_PREVIEW_OPACITY: f32 = 0.45;

#[derive(Clone)]
pub(crate) enum SidebarDragPreview {
    Space(super::space_drag::SpaceDragPreview),
    Row(super::row_drag::RowDragPreview),
}

impl Render for SidebarDrag {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let preview = match &self.preview {
            SidebarDragPreview::Space(space) => space.render(self.scale, window),
            SidebarDragPreview::Row(row) => row.render(&self.title, window),
        };
        let refused = self.refused.get();
        div()
            .when(refused, |wrapper| wrapper.opacity(REFUSED_PREVIEW_OPACITY))
            .child(preview)
    }
}

impl GhostexGpuiApp {
    pub(crate) fn update_native_sidebar_drop(
        &mut self,
        event: &DragMoveEvent<SidebarDrag>,
        target_kind: &str,
        target_id: &str,
        group_id: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let source = event.drag(cx).clone();
        self.resolve_native_sidebar_drop(
            &source,
            event.event.position,
            event.bounds,
            target_kind,
            target_id,
            group_id,
            cx,
        );
    }

    fn resolve_native_sidebar_drop(
        &mut self,
        source: &SidebarDrag,
        position: Point<Pixels>,
        bounds: Bounds<Pixels>,
        target_kind: &str,
        target_id: &str,
        group_id: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        // CDXC:Sidebar 2026-09-17 WHY:
        // GPUI broadcasts drag moves even outside a target. Only the row under the pointer may choose the drop command; otherwise later rows overwrite it.
        if !bounds.contains(&position)
            || (!matches!(target_kind, "space" | "space-row")
                && !self.native_sidebar.scroll.bounds().contains(&position))
        {
            return;
        }
        if target_kind == "space-row" && source.kind != "space" {
            return;
        }
        if source.kind == "space" && matches!(target_kind, "space" | "space-row") {
            let SidebarDragPreview::Space(space) = &source.preview else {
                return;
            };
            let command = space.drop_command(
                &source.id,
                target_kind,
                target_id,
                position,
                bounds,
                source.scale,
            );
            if self.native_sidebar.drop_command != command {
                self.native_sidebar.drop_command = command;
                cx.notify();
            }
            return;
        }
        if source.kind == target_kind && source.id == target_id {
            self.native_sidebar.drop_command = None;
            cx.notify();
            return;
        }
        let after = if target_kind == "space" {
            position.x > bounds.center().x
        } else {
            position.y > bounds.center().y
        };
        let position = if after { "after" } else { "before" };
        // A session aimed at another section of its project moves there (section_move.rs); a
        // section heading takes nothing else.
        if source.kind == "session"
            && matches!(target_kind, "section" | "session")
            && let Some(command) = self.native_sidebar_section_move_command(
                &source.id,
                target_kind,
                target_id,
                group_id,
                position,
            )
        {
            self.set_native_sidebar_session_drop(Some(command), cx);
            return;
        }
        // A project or collection over the rows under a project's header aims at the slot next to
        // that project (gx-core `project_body_drop_command`).
        if matches!(source.kind, "group" | "collection")
            && matches!(target_kind, "session" | "section" | "session-group")
        {
            let group_id = match target_kind {
                "session-group" => Some(target_id),
                _ => group_id,
            };
            let command = group_id
                .and_then(|group_id| {
                    self.gx_store_project_body_drop_command(source.kind, &source.id, group_id)
                })
                .and_then(|command| {
                    self.resolve_native_sidebar_drop_landing(command, |app, command| {
                        app.with_native_sidebar_project_drop_landing(command)
                    })
                });
            if self.native_sidebar.drop_command != command {
                self.native_sidebar.drop_command = command;
                cx.notify();
            }
            return;
        }
        if target_kind == "section" {
            // A heading of another project takes no section move from this row, but it is still
            // that project: the drop hands the session over (gx-core `check_session_handoff`).
            if source.kind == "session"
                && let Some(group_id) = group_id
            {
                self.set_native_sidebar_session_drop(
                    Some(json!({"type": "moveSession", "sessionId": source.id, "groupId": group_id, "position": "before", "handoffOnly": true})),
                    cx,
                );
                return;
            }
            if self.native_sidebar.drop_command.take().is_some() {
                cx.notify();
            }
            return;
        }
        if source.kind == "session" {
            let command = match target_kind {
                "session" => Some(
                    json!({"type": "moveSession", "sessionId": source.id, "groupId": group_id, "targetSessionId": target_id, "position": position}),
                ),
                "group" | "session-group" => Some(
                    json!({"type": "moveSession", "sessionId": source.id, "groupId": target_id, "position": "before"}),
                ),
                // CDXC:Sidebar 2026-10-23 WHY:
                // The empty list space under the last block is that block's for a session: a session dropped below a session group (often an empty one, whose header is its only row) joins it at the end. This zone took no session before, so a release there under an empty group moved nothing. A collection as the last block takes nothing, as before.
                "ungroup" => self.gx_store.sidebar_list.view().last_drawn_group_id().map(|group_id| {
                    json!({"type": "moveSession", "sessionId": source.id, "groupId": group_id, "position": "after", "zone": ghostex_gx_core::SESSION_DROP_ZONE_LIST_END})
                }),
                _ => None,
            };
            self.set_native_sidebar_session_drop(command, cx);
            return;
        }
        let command = if target_kind == "space"
            && matches!(source.kind, "group" | "collection")
            // Bots belong to no Space (gx-core `assemble`), so a Space tile takes no bot.
            && !self
                .native_sidebar
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.bots_mode)
        {
            json!({"type": "moveToSpace", "sourceKind": source.kind, "sourceId": source.id, "spaceId": target_id})
        } else if source.kind == "group" && matches!(target_kind, "collection" | "ungroup") {
            json!({"type": "moveToCollection", "sourceKind": source.kind, "sourceId": source.id, "collectionId": if target_kind == "collection" { Some(target_id) } else { None }})
        } else if source.kind == "collection" && matches!(target_kind, "group" | "collection") {
            json!({"type": "moveCollection", "sourceId": source.id, "targetKind": target_kind, "targetId": target_id, "position": position})
        } else if source.kind == target_kind {
            match target_kind {
                "group" => {
                    json!({"type": "moveGroup", "groupId": source.id, "targetGroupId": target_id, "position": position})
                }
                _ => return,
            }
        } else {
            self.native_sidebar.drop_command = None;
            cx.notify();
            return;
        };
        let command = if matches!(
            command["type"].as_str(),
            Some("moveGroup" | "moveCollection" | "moveToCollection")
        ) {
            self.resolve_native_sidebar_drop_landing(command, |app, command| {
                app.with_native_sidebar_project_drop_landing(command)
            })
        } else {
            Some(command)
        };
        if self.native_sidebar.drop_command != command {
            self.native_sidebar.drop_command = command;
            cx.notify();
        }
    }

    /// `land` for this target, or what it answered the last time the pointer was over it.
    fn resolve_native_sidebar_drop_landing(
        &mut self,
        command: Value,
        land: impl FnOnce(&mut Self, Value) -> Option<Value>,
    ) -> Option<Value> {
        if let Some((target, resolved)) = &self.native_sidebar.drop_memo
            && *target == command
        {
            return resolved.clone();
        }
        let resolved = land(self, command.clone());
        self.native_sidebar.drop_memo = Some((command, resolved.clone()));
        resolved
    }

    /// A project or collection drop with the place its row lands, from the same plan the drop
    /// performs (gx-core `sidebar_drag/project_drop.rs`); `None` for a drop that does nothing.
    fn with_native_sidebar_project_drop_landing(&mut self, command: Value) -> Option<Value> {
        let (mut command, landing) = self.gx_store_preview_project_drop(&command)?;
        if landing.unchanged {
            return None;
        }
        let row = |row: Option<ghostex_gx_core::ProjectDropRow>| {
            row.map(|row| json!({"kind": row.kind, "id": row.id}))
        };
        command["landing"] = json!({
            "collectionId": landing.collection_id,
            "after": row(landing.after),
            "before": row(landing.before),
        });
        Some(command)
    }

    /// Where a project or collection drop's line is drawn on this row's block: above the row the
    /// dropped one lands right before, or below the one it lands right after when it becomes the
    /// last among its siblings. `first` is the line above the list's first row.
    ///
    /// CDXC:Sidebar 2026-10-06 WHY:
    /// The line sits in the gap above the row it is drawn before, but the list's first row has no gap above it, so the scroll view clipped the line and a project dragged to the top of the list showed no line. That line is drawn on the first row's top edge instead.
    pub(crate) fn native_sidebar_project_drop_line(
        &self,
        kind: &str,
        id: &str,
    ) -> Option<&'static str> {
        let landing = self.native_sidebar.drop_command.as_ref()?.get("landing")?;
        let names = |row: &Value| row["kind"] == kind && row["id"] == id;
        if names(&landing["before"]) {
            let first = landing["after"].is_null() && landing["collectionId"].is_null();
            return Some(if first { "first" } else { "before" });
        }
        (landing["before"].is_null() && names(&landing["after"])).then_some("after")
    }

    /// A session drop and the line it draws, both from the store's one plan (gx-core
    /// `sidebar_drag/session_drop.rs`): the line goes where the row will land, and a drop that is
    /// refused or moves nothing keeps no command, so it draws nothing and releasing does nothing.
    fn set_native_sidebar_session_drop(&mut self, command: Option<Value>, cx: &mut Context<Self>) {
        let command = command.and_then(|command| {
            self.resolve_native_sidebar_drop_landing(command, |app, command| {
                app.with_native_sidebar_session_drop_landing(command)
            })
        });
        if self.native_sidebar.drop_command != command {
            self.native_sidebar.drop_command = command;
            cx.notify();
        }
    }

    fn with_native_sidebar_session_drop_landing(&mut self, mut command: Value) -> Option<Value> {
        let stale = |session_id: &str| {
            self.native_sidebar
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| {
                    snapshot.groups.iter().any(|group| {
                        group.is_stale
                            && group
                                .sessions
                                .iter()
                                .any(|session| session.session_id == session_id)
                    })
                })
        };
        let stale_group = |group_id: &str| {
            self.native_sidebar
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| {
                    snapshot
                        .groups
                        .iter()
                        .any(|group| group.is_stale && group.group_id == group_id)
                })
        };
        {
            let session_id = command["sessionId"].as_str()?;
            let target = command["targetSessionId"].as_str();
            if stale(session_id) || target.is_some_and(stale) {
                return None;
            }
            // The planned command, which may aim at a coordinator instead of the hovered thread,
            // is the one the drop performs.
            let drop = self.gx_store_plan_sidebar_session_drop(&command)?;
            // A drop on another project moves no row, so it draws no line: the header it goes to
            // lights up (`native_sidebar_group_drop_target`) and the release asks first.
            if let Some(handoff) = &drop.handoff {
                if stale_group(&handoff.highlight_group_id) {
                    return None;
                }
                command["handoff"] = json!({ "groupId": handoff.highlight_group_id });
                return Some(command);
            }
            let landing = drop.landing?;
            if landing.unchanged {
                return None;
            }
            let joins_group = drop
                .messages
                .iter()
                .any(|message| message["type"] == "moveSessionToGroup");
            command = drop.command;
            command["landing"] = json!({
                "groupId": landing.group_id,
                "section": landing.section,
                "afterSessionId": landing.after_session_id,
                "beforeSessionId": landing.before_session_id,
                "joinsGroup": joins_group,
            });
            Some(command)
        }
    }

    /// Where a session drop's line is drawn on this row: above the row the dropped one lands right
    /// before, or below the one it lands right after when it becomes the last of its section.
    pub(crate) fn native_sidebar_session_drop_line(
        &self,
        session_id: &str,
    ) -> Option<&'static str> {
        let landing = self.native_sidebar.drop_command.as_ref()?.get("landing")?;
        let before = landing["beforeSessionId"].as_str();
        if before == Some(session_id) {
            return Some("before");
        }
        (before.is_none() && landing["afterSessionId"].as_str() == Some(session_id))
            .then_some("after")
    }

    /// Whether a session drag would move the row into this group from another one, for the
    /// header's highlight.
    ///
    /// CDXC:Sidebar 2026-10-23 WHY:
    /// The drop line is drawn on session rows only, so a session aimed at an empty group (or at a collapsed one) showed nothing until it was released. The header lights up instead, like a section heading does for a section drop; it is read from the same plan as the line, so the two cannot disagree.
    pub(crate) fn native_sidebar_group_drop_target(&self, group_id: &str) -> bool {
        let Some(command) = self.native_sidebar.drop_command.as_ref() else {
            return false;
        };
        if let Some(handoff) = command.get("handoff") {
            return handoff["groupId"] == group_id;
        }
        command
            .get("landing")
            .is_some_and(|landing| landing["joinsGroup"] == true && landing["groupId"] == group_id)
    }

    /// CDXC:Sidebar 2026-09-22 WHY:
    /// The drop line is drawn in the gap between two rows, which is where a user aiming "between" them releases, and a release there is over no row, so no row's drop ran and the drag moved nothing while the line was still showing. The sidebar itself takes those releases and performs the drop the line shows; a row under the pointer still answers first. The line goes away once the pointer leaves the sidebar, so a release elsewhere drops nothing.
    pub(crate) fn clear_native_sidebar_drop_outside(
        &mut self,
        event: &DragMoveEvent<SidebarDrag>,
        cx: &mut Context<Self>,
    ) {
        if !event.bounds.contains(&event.event.position)
            && self.native_sidebar.drop_command.take().is_some()
        {
            cx.notify();
        }
    }

    pub(crate) fn finish_native_sidebar_drop(&mut self, cx: &mut Context<Self>) {
        if let Some(command) = self.native_sidebar.drop_command.take() {
            if command.get("handoff").is_some() {
                self.git_open_session_handoff(&command, cx);
            } else if command["type"] == "moveSessionToSection" {
                self.move_native_sidebar_session_to_section(&command, cx);
            } else {
                self.dispatch_native_sidebar_ui(command, cx);
            }
        }
        // A row drag that crossed an Agents pane hid the pane surfaces for its drop zones; a drop
        // back in the sidebar is the release the window root never sees.
        self.finish_workspace_tab_drag(cx);
        cx.notify();
    }

    pub(crate) fn native_sidebar_drop_position(&self, key: &str, id: &str) -> Option<&str> {
        let command = self.native_sidebar.drop_command.as_ref()?;
        (command.get(key)?.as_str()? == id)
            .then(|| command.get("position").and_then(Value::as_str))
            .flatten()
    }
}

pub(super) fn drop_line(position: &str, scale: f32) -> AnyElement {
    drop_line_spanning(position, scale, px(0.0), px(0.0))
}

/// The drop line of a row whose box is wider than its parent's block: `left` and `right` are how far
/// the box sits inside (positive) or outside (negative) the block's edges.
pub(super) fn drop_line_spanning(
    position: &str,
    scale: f32,
    left: gpui::Pixels,
    right: gpui::Pixels,
) -> AnyElement {
    let mut line = div()
        .absolute()
        .left(left)
        .right(right)
        .h(px(2.0 * scale))
        .bg(gpui::rgb(0x60a5fa));
    line = match position {
        "after" => line.bottom(px(-3.0 * scale)),
        "first" => line.top_0(),
        _ => line.top(px(-3.0 * scale)),
    };
    line.into_any_element()
}

pub(super) trait SidebarDropTarget:
    ParentElement + InteractiveElement + Styled + Sized
{
    fn sidebar_drop_target(
        self,
        kind: &'static str,
        id: String,
        group: Option<String>,
        cx: &mut Context<GhostexGpuiApp>,
    ) -> Self {
        let bounds = std::rc::Rc::new(std::cell::Cell::new(Bounds::default()));
        let painted_bounds = bounds.clone();
        let move_id = id.clone();
        let move_group = group.clone();
        self.relative()
            .child(
                gpui::canvas(
                    move |bounds, _, _| painted_bounds.set(bounds),
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .on_drag_move::<SidebarDrag>(cx.listener(move |app, event, _, cx| {
                app.update_native_sidebar_drop(event, kind, &move_id, move_group.as_deref(), cx);
            }))
            .on_drop::<SidebarDrag>(cx.listener(move |app, source, window, cx| {
                // Resolve again at release: a fast drag can cross its start threshold on the final move before mouse-up.
                app.native_sidebar.drop_command = None;
                app.native_sidebar.drop_memo = None;
                app.resolve_native_sidebar_drop(
                    source,
                    window.mouse_position(),
                    bounds.get(),
                    kind,
                    &id,
                    group.as_deref(),
                    cx,
                );
                app.finish_native_sidebar_drop(cx);
            }))
    }
}
impl<T: ParentElement + InteractiveElement + Styled> SidebarDropTarget for T {}
