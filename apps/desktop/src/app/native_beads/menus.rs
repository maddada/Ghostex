//! The view's small menus (a bead's ⋯, Defer, Priority, Sort, the form's Epic and Blocked by
//! pickers, the Sync panel), drawn inside the view so the desktop and the web build show the same
//! thing. They hang below where they were opened, on the sidebar menu's solid surface, and close
//! on a click outside or on Escape.

use ghostex_gx_core::beads_view::{BeadsSort, local_date_after};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Point,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, anchored, deferred, div,
    point, px,
};
use serde_json::json;

use super::view::BeadsView;
use crate::app::helpers::titlebar_svg_icon;
use crate::app::native_kanban::palette::KanbanPalette;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BeadsMenuKind {
    Row(String),
    Defer(String),
    Priority(String),
    Sort,
    CreateEpic,
    CreateBlockedBy,
    Sync,
}

#[derive(Clone, Debug)]
pub(crate) struct BeadsMenu {
    pub(crate) kind: BeadsMenuKind,
    pub(crate) position: Point<Pixels>,
}

impl BeadsMenu {
    pub(crate) fn new(kind: BeadsMenuKind, position: Point<Pixels>) -> Self {
        Self { kind, position }
    }
}

struct Entry {
    label: String,
    icon: Option<&'static str>,
    checked: bool,
    disabled: bool,
    run: Option<MenuRun>,
}

type MenuRun = Box<dyn Fn(&mut BeadsView, &mut Window, &mut Context<BeadsView>)>;

fn entry(
    label: impl Into<String>,
    icon: Option<&'static str>,
    run: impl Fn(&mut BeadsView, &mut Window, &mut Context<BeadsView>) + 'static,
) -> Entry {
    Entry {
        label: label.into(),
        icon,
        checked: false,
        disabled: false,
        run: Some(Box::new(run)),
    }
}

impl BeadsView {
    fn menu_entries(&self, kind: &BeadsMenuKind) -> Vec<Entry> {
        use ghostex_gx_protocol::beads_view::capability;
        match kind {
            BeadsMenuKind::Row(id) => {
                let Some(row) = self.row(id).cloned() else {
                    return Vec::new();
                };
                let mut entries = Vec::new();
                let closed = row.status == "closed";
                if !closed && row.assignee.is_none() && self.has_capability(capability::CLAIM) {
                    let id = id.clone();
                    entries.push(entry(
                        "Claim",
                        Some("titlebar/user.svg"),
                        move |this, window, cx| {
                            this.simple_action(
                                "claim",
                                &id,
                                json!({}),
                                "Couldn't claim this bead",
                                window,
                                cx,
                            )
                        },
                    ));
                }
                if closed {
                    if self.has_capability(capability::REOPEN) {
                        let id = id.clone();
                        entries.push(entry(
                            "Reopen",
                            Some("titlebar/rotate-clockwise.svg"),
                            move |this, window, cx| {
                                this.simple_action(
                                    "reopen",
                                    &id,
                                    json!({}),
                                    "Couldn't reopen this bead",
                                    window,
                                    cx,
                                )
                            },
                        ));
                    }
                } else {
                    let id_close = id.clone();
                    entries.push(entry(
                        "Close",
                        Some("titlebar/check.svg"),
                        move |this, window, cx| {
                            this.simple_action(
                                "close",
                                &id_close,
                                json!({}),
                                "Couldn't close this bead",
                                window,
                                cx,
                            )
                        },
                    ));
                    if row.status == "deferred" && self.has_capability(capability::DEFER) {
                        let id = id.clone();
                        entries.push(entry(
                            "Bring back",
                            Some("titlebar/player-play.svg"),
                            move |this, window, cx| {
                                this.simple_action(
                                    "undefer",
                                    &id,
                                    json!({}),
                                    "Couldn't bring this bead back",
                                    window,
                                    cx,
                                )
                            },
                        ));
                    }
                }
                if row.needs_decision && self.has_capability(capability::HUMAN) {
                    let id = id.clone();
                    entries.push(entry(
                        "Dismiss the decision",
                        Some("titlebar/x.svg"),
                        move |this, window, cx| {
                            this.simple_action(
                                "humanDismiss",
                                &id,
                                json!({}),
                                "Couldn't dismiss this decision",
                                window,
                                cx,
                            )
                        },
                    ));
                }
                let copy = id.clone();
                entries.push(entry(
                    "Copy ID",
                    Some("titlebar/copy.svg"),
                    move |this, window, cx| {
                        (this.host.clone())(
                            super::view::BeadsRequest::Copy(copy.clone()),
                            window,
                            cx,
                        )
                    },
                ));
                entries
            }
            // CDXC:ProjectBoard 2026-11-11 DECISION: User (the approved mockup): Defer offers Tomorrow, Next week, Pick a date… (the app's date picker) and On ice, which waits until Bring back.
            BeadsMenuKind::Defer(id) => {
                let clock = self.clock;
                let presets = [
                    ("Tomorrow", Some(local_date_after(1, &clock))),
                    ("Next week", Some(local_date_after(7, &clock))),
                ];
                let mut entries: Vec<Entry> = presets
                    .into_iter()
                    .map(|(label, until)| {
                        let id = id.clone();
                        entry(
                            label,
                            Some("titlebar/clock.svg"),
                            move |this, window, cx| this.defer(&id, until.clone(), window, cx),
                        )
                    })
                    .collect();
                let pick = id.clone();
                entries.push(entry(
                    "Pick a date…",
                    Some("titlebar/clock-check.svg"),
                    move |this, window, cx| this.open_defer_picker(&pick, window, cx),
                ));
                let ice = id.clone();
                entries.push(entry(
                    "On ice",
                    Some("titlebar/player-pause.svg"),
                    move |this, window, cx| this.defer(&ice, None, window, cx),
                ));
                entries
            }
            BeadsMenuKind::Priority(id) => (0..=4)
                .map(|priority| {
                    let id = id.clone();
                    let current = self.row(&id).and_then(|row| row.priority) == Some(priority);
                    let mut item = entry(format!("P{priority}"), None, move |this, window, cx| {
                        this.simple_action(
                            "updatePriority",
                            &id,
                            json!({ "priority": priority.to_string() }),
                            "Couldn't change the priority",
                            window,
                            cx,
                        )
                    });
                    item.checked = current;
                    item
                })
                .collect(),
            BeadsMenuKind::Sort => [BeadsSort::Priority, BeadsSort::LastChanged]
                .into_iter()
                .map(|sort| {
                    let mut item = entry(sort.label(), None, move |this, _, cx| {
                        this.set_sort(sort, cx)
                    });
                    item.checked = self.options.sort == sort;
                    item
                })
                .collect(),
            BeadsMenuKind::CreateEpic => {
                let current = self.create.as_ref().and_then(|form| form.epic.clone());
                let mut entries = vec![{
                    let mut item = entry("No epic", None, |this, _, cx| {
                        this.set_create_epic(None, cx)
                    });
                    item.checked = current.is_none();
                    item
                }];
                for row in self
                    .rows
                    .iter()
                    .filter(|row| row.issue_type == "epic" && row.status != "closed")
                {
                    let id = row.id.clone();
                    let mut item = entry(
                        row.title.clone(),
                        Some("titlebar/stack.svg"),
                        move |this, _, cx| this.set_create_epic(Some(id.clone()), cx),
                    );
                    item.checked = current.as_deref() == Some(row.id.as_str());
                    entries.push(item);
                }
                entries
            }
            BeadsMenuKind::CreateBlockedBy => {
                let chosen = self
                    .create
                    .as_ref()
                    .map(|form| form.blocked_by.clone())
                    .unwrap_or_default();
                self.rows
                    .iter()
                    .filter(|row| row.status != "closed" && row.issue_type != "epic")
                    .take(60)
                    .map(|row| {
                        let id = row.id.clone();
                        let mut item = entry(row.title.clone(), None, move |this, _, cx| {
                            this.toggle_create_blocker(&id, cx)
                        });
                        item.checked = chosen.contains(&row.id);
                        item
                    })
                    .collect()
            }
            BeadsMenuKind::Sync => Vec::new(),
        }
    }

    pub(crate) fn render_menu(
        &mut self,
        p: &KanbanPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.menu.clone()?;
        let keeps_open = matches!(menu.kind, BeadsMenuKind::CreateBlockedBy);
        let body = if menu.kind == BeadsMenuKind::Sync {
            self.render_sync_panel(p, window, cx)
        } else {
            let entries = self.menu_entries(&menu.kind);
            if entries.is_empty() {
                return None;
            }
            let hover = p.card_hover;
            div()
                .flex()
                .flex_col()
                .py(px(4.0))
                .min_w(px(180.0))
                .max_w(px(320.0))
                .max_h(px(360.0))
                .id("beads-menu-rows")
                .overflow_y_scroll()
                .children(entries.into_iter().enumerate().map(|(index, entry)| {
                    let Entry {
                        label,
                        icon,
                        checked,
                        disabled,
                        run,
                    } = entry;
                    div()
                        .id(SharedString::from(format!("beads-menu-row-{index}")))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .mx(px(4.0))
                        .px(px(8.0))
                        .h(px(28.0))
                        .rounded(px(6.0))
                        .text_size(px(13.0))
                        .text_color(p.foreground)
                        .when(disabled, |this| this.opacity(0.45))
                        .when(!disabled, |this| {
                            this.cursor_pointer().hover(move |style| style.bg(hover))
                        })
                        .child(
                            div()
                                .w(px(14.0))
                                .flex_none()
                                .when(checked, |this| {
                                    this.child(titlebar_svg_icon(
                                        "titlebar/check.svg",
                                        13.0,
                                        p.foreground,
                                    ))
                                })
                                .when(!checked, |this| {
                                    this.when_some(icon, |this, icon| {
                                        this.child(titlebar_svg_icon(icon, 13.0, p.muted))
                                    })
                                }),
                        )
                        .child(div().truncate().child(label))
                        .when_some(run.filter(|_| !disabled), |this, run| {
                            this.on_click(cx.listener(move |this, _, window, cx| {
                                if !keeps_open {
                                    this.menu = None;
                                }
                                run(this, window, cx);
                                cx.notify();
                            }))
                        })
                }))
                .into_any_element()
        };
        let position = point(menu.position.x, menu.position.y + px(6.0));
        Some(
            deferred(
                anchored().position(position).snap_to_window().child(
                    div()
                        .id("beads-menu")
                        .occlude()
                        .bg(super::render::solid_surface(p))
                        .border_1()
                        .border_color(p.border_strong)
                        .rounded(px(10.0))
                        .shadow_lg()
                        .font_family(p.font.clone())
                        .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_menu(cx)))
                        .child(body),
                ),
            )
            .with_priority(3)
            .into_any_element(),
        )
    }
}
