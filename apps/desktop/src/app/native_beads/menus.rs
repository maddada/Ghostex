//! The view's small menus (a bead's ⋯, Defer, Priority, Sort, the form's Epic and Blocked by
//! pickers, Sync), shown with the sidebar's own menu (`native_sidebar/menus.rs`: its rows, its
//! frosted host on the desktop, its dismissal), kept inside the Beads view's area. A row runs the
//! entry this file built for it.

use std::cell::Cell;
use std::rc::Rc;

use ghostex_gx_core::beads_view::{BeadsSort, local_date_after, parse_bd_time_ms, relative_ago};
use gpui::{Bounds, Context, Pixels, Point, Window, point, px, size};
use serde_json::{Value, json};

use super::view::{BeadsNotice, BeadsTarget, BeadsView};
use crate::GhostexGpuiApp;
use crate::app::native_sidebar::menus::SidebarMenuOpener;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BeadsMenuKind {
    Row(String),
    Defer(String),
    Priority(String),
    Sort,
    /// The narrow filter row's Sort and Show done.
    ViewOptions,
    CreateEpic,
    CreateBlockedBy,
    Sync,
}

#[derive(Clone, Debug)]
pub(crate) struct BeadsMenu {
    pub(crate) kind: BeadsMenuKind,
    pub(crate) position: Point<Pixels>,
    /// Set when the menu is asked for; the next draw hands it to the sidebar's menu.
    pub(crate) pending: bool,
    /// Which opening a row belongs to, so a stale row never runs.
    pub(crate) serial: u64,
    /// The project and workspace generation the menu was opened for: a row runs only while both
    /// still hold, so it never writes to another project or workspace.
    pub(crate) target: Option<BeadsTarget>,
    pub(crate) generation: u64,
}

impl BeadsMenu {
    pub(crate) fn new(kind: BeadsMenuKind, position: Point<Pixels>) -> Self {
        Self {
            kind,
            position,
            pending: true,
            serial: 0,
            target: None,
            generation: 0,
        }
    }
}

/// A Project board diagnostic line about the view's menus and writes (open, reset, dispatch,
/// result), written only while that diagnostic area is on.
pub(crate) fn menu_log(event: &str, details: Value) {
    crate::support_logs::append(
        crate::support_logs::GpuiSupportLog::ProjectBoard,
        event,
        details,
    );
}

/// Where the Beads view was last drawn, the area its menus stay inside.
pub(crate) type ViewBounds = Rc<Cell<Option<Bounds<Pixels>>>>;

struct Entry {
    label: String,
    icon: Option<&'static str>,
    checked: bool,
    disabled: bool,
    run: Option<MenuRun>,
}

pub(crate) type MenuRun = Box<dyn Fn(&mut BeadsView, &mut Window, &mut Context<BeadsView>)>;

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
                    entries.push(entry("Claim", Some("user"), move |this, window, cx| {
                        this.simple_action(
                            "claim",
                            &id,
                            json!({}),
                            "Couldn't claim this bead",
                            window,
                            cx,
                        )
                    }));
                }
                if closed {
                    if self.has_capability(capability::REOPEN) {
                        let id = id.clone();
                        entries.push(entry(
                            "Reopen",
                            Some("rotateClockwise"),
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
                        Some("circleCheck"),
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
                            Some("playerPlay"),
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
                        Some("x"),
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
                entries.push(entry("Copy ID", Some("copy"), move |_, _, cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy.clone()))
                }));
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
                        entry(label, Some("clock"), move |this, window, cx| {
                            this.defer(&id, until.clone(), window, cx)
                        })
                    })
                    .collect();
                let pick = id.clone();
                entries.push(entry(
                    "Pick a date…",
                    Some("clockCheck"),
                    move |this, window, cx| this.open_defer_picker(&pick, window, cx),
                ));
                let ice = id.clone();
                entries.push(entry(
                    "On ice",
                    Some("playerPause"),
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
            BeadsMenuKind::ViewOptions => {
                let mut entries = vec![{
                    let mut item =
                        entry("Show done", None, |this, _, cx| this.toggle_show_done(cx));
                    item.checked = self.options.show_done;
                    item
                }];
                for sort in [BeadsSort::Priority, BeadsSort::LastChanged] {
                    let mut item = entry(
                        format!("Sort by {}", sort.label().to_lowercase()),
                        None,
                        move |this, _, cx| this.set_sort(sort, cx),
                    );
                    item.checked = self.options.sort == sort;
                    entries.push(item);
                }
                entries
            }
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
                    let mut item = entry(row.title.clone(), Some("stack"), move |this, _, cx| {
                        this.set_create_epic(Some(id.clone()), cx)
                    });
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

    /// Hands a menu asked for since the last draw to the sidebar's menu.
    pub(crate) fn show_pending_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.menu.as_mut().filter(|menu| menu.pending) else {
            return;
        };
        menu.pending = false;
        self.menu_serial += 1;
        menu.serial = self.menu_serial;
        menu.target = self.target.clone();
        menu.generation = self.generation;
        let menu = menu.clone();
        menu_log(
            "beadsView.menuOpen",
            json!({
                "kind": format!("{:?}", menu.kind),
                "serial": menu.serial,
                "generation": menu.generation,
                "project": menu.target.as_ref().map(|target| target.project_key.clone()),
            }),
        );
        let items: Vec<Value> = if menu.kind == BeadsMenuKind::Sync {
            self.menu_runs.clear();
            self.sync_items(menu.serial)
        } else {
            // CDXC:ProjectBoard 2026-11-11 WHY: each row keeps the action and the bead it showed when the menu opened; a poll that changes the bead meanwhile (someone claims it) must never turn a shown "Claim" into whatever now sits at that position.
            let entries = self.menu_entries(&menu.kind);
            let mut runs = Vec::with_capacity(entries.len());
            let items = entries
                .into_iter()
                .enumerate()
                .map(|(index, mut entry)| {
                    runs.push(entry.run.take());
                    let mut item = json!({
                        "label": entry.label,
                        "command": { "type": "beadsMenu", "serial": menu.serial, "index": index },
                    });
                    if let Some(icon) = entry.icon {
                        item["icon"] = json!(icon);
                    }
                    if entry.checked {
                        item["checked"] = json!(true);
                    }
                    if entry.disabled {
                        item["disabled"] = json!(true);
                    }
                    item
                })
                .collect();
            self.menu_runs = runs;
            items
        };
        if items.is_empty() {
            self.menu = None;
            return;
        }
        let owner = Bounds {
            origin: point(menu.position.x - px(1.0), menu.position.y - px(12.0)),
            size: size(px(2.0), px(24.0)),
        };
        let region = self
            .bounds
            .get()
            .unwrap_or_else(|| Bounds::new(point(px(0.0), px(0.0)), window.viewport_size()));
        let opener = SidebarMenuOpener::below(owner)
            .toggled_by(format!("beads-menu-{:?}", menu.kind), Some(menu.position));
        GhostexGpuiApp::show_native_menu_in_region(
            &Value::Array(items),
            opener,
            region,
            window,
            cx,
        );
    }

    /// A row of a menu this view opened was picked (`type: beadsMenu`).
    pub(crate) fn run_menu_command(
        &mut self,
        command: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let serial = command["serial"].as_u64();
        let Some(menu) = self.menu.take() else {
            // The view started over (another project or workspace) while the menu was open.
            let stale = serial.is_some_and(|serial| serial == self.menu_serial);
            menu_log(
                "beadsView.menuDispatch",
                json!({ "serial": serial, "outcome": if stale { "staleAfterReset" } else { "noMenu" } }),
            );
            if stale {
                self.menu_runs.clear();
                self.notice = Some(stale_menu_notice());
                cx.notify();
            }
            return;
        };
        if serial != Some(menu.serial) {
            menu_log(
                "beadsView.menuDispatch",
                json!({ "serial": serial, "open": menu.serial, "outcome": "otherMenu" }),
            );
            return;
        }
        if menu.generation != self.generation || menu.target != self.target {
            menu_log(
                "beadsView.menuDispatch",
                json!({ "serial": serial, "outcome": "targetChanged" }),
            );
            self.menu_runs.clear();
            self.notice = Some(stale_menu_notice());
            cx.notify();
            return;
        }
        menu_log(
            "beadsView.menuDispatch",
            json!({ "serial": serial, "outcome": "run", "index": command["index"] }),
        );
        let Some(index) = command["index"].as_u64().map(|index| index as usize) else {
            return;
        };
        if menu.kind == BeadsMenuKind::Sync {
            match command["action"].as_str() {
                Some("pull") => self.start_sync("pull", window, cx),
                Some("push") => self.start_sync("push", window, cx),
                _ => {}
            }
            return;
        }
        let runs = std::mem::take(&mut self.menu_runs);
        if let Some(run) = runs.into_iter().nth(index).flatten() {
            run(self, window, cx);
        }
        cx.notify();
    }

    /// Sync: the remote, the last push and pull, then Pull and Push.
    fn sync_items(&self, serial: u64) -> Vec<Value> {
        let sync = self
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.sync.clone())
            .unwrap_or_default();
        let clock = self.clock;
        let ago = |time: &Option<String>| {
            time.as_deref()
                .and_then(parse_bd_time_ms)
                .map(|time| relative_ago(time, &clock))
                .unwrap_or_else(|| "not yet".to_string())
        };
        let running = sync.running.is_some();
        let mut items = vec![
            json!({ "heading": "Sync" }),
            json!({
                "label": match sync.remote.as_ref() {
                    Some(remote) => format!("Remote: {} {}", remote.name, remote.url),
                    None => "No remote set up".to_string(),
                },
                "icon": "cloud",
                "disabled": true,
            }),
            json!({ "label": format!("Last push: {}", ago(&sync.last_push_at)), "disabled": true }),
            json!({ "label": format!("Last pull: {}", ago(&sync.last_pull_at)), "disabled": true }),
            json!({
                "label": "Push sends this computer's changes. Pull brings in changes from your other computers.",
                "disabled": true,
            }),
        ];
        if sync.remote.is_some() {
            items.push(json!({ "separator": true }));
            for (action, label, icon) in [("pull", "Pull", "download"), ("push", "Push", "upload")]
            {
                items.push(json!({
                    "label": label,
                    "icon": icon,
                    "disabled": running,
                    "command": { "type": "beadsMenu", "serial": serial, "index": 0, "action": action },
                }));
            }
        }
        items
    }
}

/// The card for a menu pick that no longer applies.
fn stale_menu_notice() -> BeadsNotice {
    BeadsNotice {
        title: "Nothing was changed".to_string(),
        message: "The list changed while the menu was open. Open it again and pick once more."
            .to_string(),
        details: None,
        pull_now: false,
    }
}
