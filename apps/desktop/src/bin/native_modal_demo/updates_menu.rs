//! Preview of the top bar's Updates button and list (app/titlebar/updates_menu_rows.rs, drawn
//! from the app's own file): one item updating, one failed with its reason on the red icon, one
//! waiting. `GHOSTEX_NATIVE_MODAL_DEMO_STATE=dismissed` shows the list after the failure was
//! dismissed. Clicks print what the app would run.
use gpui::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Window, div, px, rgb,
};
use gpui_component::h_flex;
use gpui_component::menu::{PopupMenu, PopupMenuAppearance};
use serde_json::{Value, json};

use crate::app::actions::*;
use crate::app::helpers::*;

#[path = "../../app/titlebar/updates_menu_rows.rs"]
mod rows;

fn items(dismissed: bool) -> Vec<Value> {
    let mut items = vec![
        json!({
            "id": "agent:codex", "name": "Codex", "state": "updating",
            "progress": "Updating…", "canUpdate": false, "counts": true,
        }),
        json!({
            "id": "app", "name": "Ghostex", "state": "available",
            "latest": "10.4.0", "canUpdate": true, "counts": true,
        }),
        // Another computer's items name it (app/updates/remote.rs).
        json!({
            "id": "remote:remote-linux-box:agent:opencode", "kind": "agent", "name": "OpenCode",
            "state": "available", "latest": "2.2.5", "canUpdate": true, "counts": true,
            "computer": "remote-linux-box", "computerName": "Linux box",
        }),
        json!({
            "id": "remote:remote-linux-box:agent:claude", "kind": "agent", "name": "Claude Code",
            "state": "updating", "progress": "Waiting…", "canUpdate": false, "counts": true,
            "computer": "remote-linux-box", "computerName": "Linux box",
        }),
    ];
    if !dismissed {
        items.insert(0, json!({
            "id": "agent:claude", "name": "Claude Code", "state": "failed",
            "canUpdate": true, "counts": true,
            "error": "npm couldn't write to /usr/local/lib/node_modules (permission denied). Fix that folder's owner, or update Claude Code in Settings > Agents.",
        }));
    }
    items
}

struct UpdatesMenuDemo {
    menu: Entity<PopupMenu>,
    items: Vec<Value>,
}

impl UpdatesMenuDemo {
    fn new(dismissed: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let items = items(dismissed);
        let rows = items.clone();
        let menu = PopupMenu::build(window, cx, move |menu, _, _| {
            let mut menu = menu
                .appearance(PopupMenuAppearance {
                    shadow: false,
                    panel_radius: px(8.0),
                    item_radius: px(6.0),
                    padding: px(6.0),
                    item_padding_x: px(10.0),
                    item_height: px(34.0),
                    separator_margin: px(6.0),
                    background: titlebar_popup_menu_background(),
                    foreground: titlebar_popup_menu_foreground(),
                    border: titlebar_popup_menu_border_color(),
                    hover: titlebar_popup_menu_hover_color(),
                })
                .min_w(px(300.0))
                .max_w(px(300.0));
            let failed = rows.iter().filter(|item| item["state"] == "failed").count();
            let count = rows.len();
            let other_computers = rows.iter().any(|item| item["computerName"].is_string());
            menu = menu.menu_element_with_disabled(
                Box::new(RunGpuiTitlebarUpdates { update_all: false }),
                true,
                move |_, _| rows::updates_menu_header(count, failed, other_computers),
            );
            for item in rows.clone() {
                let id = item["id"].as_str().unwrap_or_default().to_string();
                menu = menu
                    .menu_element(Box::new(RunGpuiTitlebarUpdateItem { id }), move |_, _| {
                        rows::updates_menu_item_row(&item)
                    });
            }
            menu.separator()
                .menu_element(
                    Box::new(RunGpuiTitlebarUpdates { update_all: true }),
                    |_, _| rows::updates_menu_update_all_row(),
                )
                .menu_element(
                    Box::new(RunGpuiTitlebarUpdates { update_all: false }),
                    |_, _| {
                        titlebar_popup_standard_menu_row(
                            "titlebar/settings.svg",
                            14.0,
                            "Open Updates…".to_string(),
                            false,
                        )
                    },
                )
        });
        menu.focus_handle(cx).focus(window, cx);
        Self { menu, items }
    }
}

impl Render for UpdatesMenuDemo {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let failed = self.items.iter().any(|item| item["state"] == "failed");
        let busy = self.items.iter().any(|item| item["state"] == "updating");
        div()
            .size_full()
            .bg(rgb(0x3a3c42))
            .p(px(10.0))
            .on_action(|action: &RunGpuiTitlebarUpdateItem, _, _| {
                eprintln!("run update of {}", action.id)
            })
            .on_action(|action: &DismissGpuiTitlebarUpdateFailure, _, _| {
                eprintln!("dismiss failure of {}: {}", action.id, action.error)
            })
            .on_action(|action: &RunGpuiTitlebarUpdates, _, _| {
                eprintln!("updates: update_all={}", action.update_all)
            })
            .child(
                h_flex()
                    .h(px(34.0))
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .h(px(30.0))
                            .px(px(8.0))
                            .items_center()
                            .rounded(px(7.0))
                            .when(failed, |this| {
                                this.bg(rows::update_button_failed_background())
                            })
                            .child(rows::update_button_face(busy, failed, self.items.len())),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(titlebar_active_text_color())
                            .child("Ghostex"),
                    ),
            )
            .child(div().mt(px(4.0)).w(px(302.0)).child(self.menu.clone()))
    }
}

use gpui::Focusable as _;
use gpui::prelude::FluentBuilder as _;

const WIDTH: f32 = 460.0;
const HEIGHT: f32 = 400.0;

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let dismissed = demo.state == "dismissed";
    // The env reader lowercases; a snapshot path keeps its case.
    let snapshot = std::env::var("GHOSTEX_NATIVE_MODAL_DEMO_SNAPSHOT").unwrap_or_default();
    if !snapshot.trim().is_empty() {
        open_snapshot(dismissed, snapshot.trim().to_string(), cx);
        return;
    }
    super::open_modal_window(
        WIDTH,
        HEIGHT,
        move |window, cx| cx.new(|cx| UpdatesMenuDemo::new(dismissed, window, cx)),
        cx,
    );
}

/// Draws the preview in a window far off every screen, optionally points at the HOVER spot (a
/// failure icon, for its tooltip), then writes the frame to `path` and quits.
#[cfg(feature = "modal-demo-snapshot")]
fn open_snapshot(dismissed: bool, path: String, cx: &mut App) {
    use gpui::{Bounds, Modifiers, MouseMoveEvent, PlatformInput, WindowBounds, WindowOptions};
    use gpui::{point, size};
    use std::time::Duration;

    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            point(px(-10000.0), px(-10000.0)),
            size(px(WIDTH), px(HEIGHT)),
        ))),
        focus: false,
        show: true,
        titlebar: None,
        ..Default::default()
    };
    let window = cx
        .open_window(options, move |window, cx| {
            let view = cx.new(|cx| UpdatesMenuDemo::new(dismissed, window, cx));
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        })
        .expect("open the snapshot window");
    let hover = std::env::var("GHOSTEX_NATIVE_MODAL_DEMO_HOVER")
        .ok()
        .and_then(|spot| {
            let (x, y) = spot.split_once(',')?;
            Some(point(
                px(x.trim().parse().ok()?),
                px(y.trim().parse().ok()?),
            ))
        });
    cx.spawn(async move |cx| {
        cx.background_executor()
            .timer(Duration::from_millis(800))
            .await;
        if let Some(position) = hover {
            // Through the plain window handle: a typed update would hold the root the event reads.
            let _ = cx.update_window(window.into(), |_, window, cx| {
                window.dispatch_event(
                    PlatformInput::MouseMove(MouseMoveEvent {
                        position,
                        pressed_button: None,
                        modifiers: Modifiers::default(),
                    }),
                    cx,
                );
                window.refresh();
            });
            cx.background_executor()
                .timer(Duration::from_millis(1500))
                .await;
        }
        let _ = window.update(cx, |_, window, _| {
            window.refresh();
        });
        cx.background_executor()
            .timer(Duration::from_millis(300))
            .await;
        let _ = window.update(cx, |_, window, _| match window.render_to_image() {
            Ok(image) => match image.save(&path) {
                Ok(()) => eprintln!("wrote {path}"),
                Err(error) => eprintln!("couldn't write {path}: {error}"),
            },
            Err(error) => eprintln!("couldn't draw the snapshot: {error}"),
        });
        let _ = cx.update(|cx| cx.quit());
    })
    .detach();
}

#[cfg(not(feature = "modal-demo-snapshot"))]
fn open_snapshot(_dismissed: bool, _path: String, cx: &mut App) {
    eprintln!("snapshots need: cargo build --bin native-modal-demo --features modal-demo-snapshot");
    cx.quit();
}
