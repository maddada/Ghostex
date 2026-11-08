//! Click-through check of the Start menu against a throwaway gxserver (`cargo xtask test-server`),
//! laid out like the app: a main window and the menu in its own popup window, whose row clicks
//! reach the main window only through the app's own `on_start_menu_picks`
//! (app/titlebar/actions_menu_rows.rs), the list the app's popup window registers.
//!
//! Inputs (source the test server's env.sh first, so `ghostex` talks to it):
//! - `GHOSTEX_START_MENU_DEMO_HUD`: a file holding that server's `/api/readSidebarHud` answer.
//! - `GHOSTEX_START_MENU_DEMO_PROJECT`: the project the menu belongs to.
//! - `GHOSTEX_START_MENU_DEMO_CLICK=x,y`: a point in the popup window to click.
//! - `GHOSTEX_START_MENU_DEMO_UNWIRED=1`: leave the popup's listeners out (the 11.1.0 bug).
//! - `GHOSTEX_NATIVE_MODAL_DEMO_SNAPSHOT=<png>`: draw both windows offscreen (`<png>` and
//!   `<png>-popup.png`) after the click, then quit.
//!
//! A Global row's click is resolved by gx-core's `plan_sidebar_command_run` on that HUD, as the app
//! does, and a resolved Terminal Action is started on the test server with
//! `ghostex run-action <id> --project-id <project> --scope global`.
use gpui::{
    App, AppContext as _, Context, Entity, Focusable as _, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, WeakEntity, Window, div, px, rgb,
};
use gpui_component::Side;
use gpui_component::menu::{PopupMenu, PopupMenuAppearance};
use serde_json::{Value, json};

use crate::app::helpers::*;

#[path = "../../app/titlebar/actions_menu_rows.rs"]
mod rows;

use rows::{StartMenuPick, on_start_menu_picks};

fn read_hud() -> Value {
    let path = std::env::var("GHOSTEX_START_MENU_DEMO_HUD").unwrap_or_default();
    let text = std::fs::read_to_string(path.trim()).unwrap_or_default();
    let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    value["result"].clone()
}

fn actions_from(rows: &Value) -> Vec<GpuiTitlebarAction> {
    rows.as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|row| GpuiTitlebarAction {
            command_id: row["commandId"].as_str().unwrap_or_default().to_string(),
            name: row["name"].as_str().unwrap_or_default().to_string(),
            icon_path: "titlebar/player-play.svg",
            preview: row["command"]
                .as_str()
                .or(row["url"].as_str())
                .unwrap_or_default()
                .to_string(),
        })
        .collect()
}

/// The main window: what each forwarded click did.
struct Harness {
    hud: Value,
    project: String,
    log: Vec<String>,
}

impl Harness {
    fn note(&mut self, line: String, cx: &mut Context<Self>) {
        eprintln!("{line}");
        self.log.push(line);
        cx.notify();
    }

    fn on_pick(&mut self, pick: StartMenuPick, cx: &mut Context<Self>) {
        self.note(format!("main window heard: {pick:?}"), cx);
        let StartMenuPick::Global(command_id) = pick else {
            return;
        };
        let plan = ghostex_gx_core::plan_sidebar_command_run(
            Some(&self.hud),
            &json!({ "commandId": command_id, "scope": "global" }),
            None,
        );
        let action = match plan {
            ghostex_gx_core::SidebarCommandRun::Run { action, .. } => action,
            other => {
                self.note(
                    format!("plan: {other:?} (the app shows an error toast)"),
                    cx,
                );
                return;
            }
        };
        self.note(
            format!(
                "plan: Run {} \"{}\": {}",
                action["actionType"].as_str().unwrap_or_default(),
                action["name"].as_str().unwrap_or_default(),
                action["command"].as_str().unwrap_or_default()
            ),
            cx,
        );
        let output = std::process::Command::new("ghostex")
            .args([
                "run-action",
                &command_id,
                "--project-id",
                &self.project,
                "--scope",
                "global",
            ])
            .output();
        match output {
            Ok(output) => {
                let text = String::from_utf8_lossy(&output.stdout);
                let parsed: Value = serde_json::from_str(text.trim()).unwrap_or(Value::Null);
                let session = &parsed["session"];
                self.note(
                    format!(
                        "terminal: ok={} session={} title={} cwd={}",
                        parsed["ok"],
                        session["sessionId"].as_str().unwrap_or("-"),
                        session["title"].as_str().unwrap_or("-"),
                        session["cwd"]
                            .as_str()
                            .or(session["projectPath"].as_str())
                            .unwrap_or("-")
                    ),
                    cx,
                );
            }
            Err(error) => self.note(format!("terminal: couldn't run ghostex: {error}"), cx),
        }
    }
}

impl Render for Harness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let lines = if self.log.is_empty() {
            vec!["main window heard nothing".to_string()]
        } else {
            self.log.clone()
        };
        div()
            .size_full()
            .bg(rgb(0x1f2024))
            .p(px(12.0))
            .text_size(px(12.0))
            .text_color(titlebar_popup_menu_foreground())
            .children(lines.into_iter().map(|line| div().py(px(2.0)).child(line)))
    }
}

/// The popup window: the menu, and (unless unwired) the app's listener list.
struct Popup {
    menu: Entity<PopupMenu>,
    main: WeakEntity<Harness>,
    wired: bool,
}

impl Popup {
    fn new(
        hud: &Value,
        main: WeakEntity<Harness>,
        wired: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let actions = actions_from(&hud["commands"]);
        let globals = actions_from(&hud["globalCommands"]);
        let menu = PopupMenu::build(window, cx, move |menu, _, _| {
            let menu = menu
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
                .min_w(px(280.0))
                .max_w(px(280.0))
                .check_side(Side::Right);
            let active = actions.first().map(|action| action.command_id.clone());
            rows::titlebar_actions_menu_rows(menu, &actions, active.as_deref(), &globals)
        });
        menu.focus_handle(cx).focus(window, cx);
        Self { menu, main, wired }
    }
}

impl Render for Popup {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let main = self.main.clone();
        let root = div().id("start-menu-popup").size_full().bg(rgb(0x3a3c42));
        let root = if self.wired {
            on_start_menu_picks(root, move |pick, _window, cx| {
                let pick = pick.clone();
                let _ = main.update(cx, |harness, cx| harness.on_pick(pick, cx));
            })
        } else {
            root
        };
        root.child(self.menu.clone())
    }
}

pub(super) fn open(_demo: &super::DemoEnv, cx: &mut App) {
    open_windows(cx);
}

#[cfg(feature = "modal-demo-snapshot")]
fn open_windows(cx: &mut App) {
    use gpui::{
        Bounds, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
        PlatformInput, WindowBounds, WindowOptions, point, size,
    };
    use std::time::Duration;

    let hud = read_hud();
    let project = std::env::var("GHOSTEX_START_MENU_DEMO_PROJECT").unwrap_or_default();
    let wired = std::env::var("GHOSTEX_START_MENU_DEMO_UNWIRED").as_deref() != Ok("1");
    let snapshot = std::env::var("GHOSTEX_NATIVE_MODAL_DEMO_SNAPSHOT").unwrap_or_default();
    let click = std::env::var("GHOSTEX_START_MENU_DEMO_CLICK")
        .ok()
        .and_then(|spot| {
            let (x, y) = spot.split_once(',')?;
            Some(point(
                px(x.trim().parse().ok()?),
                px(y.trim().parse().ok()?),
            ))
        });
    let offscreen = |x: f32, width: f32, height: f32| WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            point(px(x), px(-10000.0)),
            size(px(width), px(height)),
        ))),
        focus: false,
        show: true,
        titlebar: None,
        ..Default::default()
    };
    let main_view = std::rc::Rc::new(std::cell::RefCell::new(None));
    let main_slot = main_view.clone();
    let main_hud = hud.clone();
    let main_window = cx
        .open_window(offscreen(-10000.0, 640.0, 160.0), move |window, cx| {
            let view = cx.new(|_| Harness {
                hud: main_hud,
                project,
                log: Vec::new(),
            });
            *main_slot.borrow_mut() = Some(view.downgrade());
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        })
        .expect("open the main window");
    let main = main_view.borrow().clone().expect("main view");
    let popup_window = cx
        .open_window(offscreen(-9000.0, 300.0, 520.0), move |window, cx| {
            let view = cx.new(|cx| Popup::new(&hud, main, wired, window, cx));
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        })
        .expect("open the popup window");
    cx.spawn(async move |cx| {
        cx.background_executor()
            .timer(Duration::from_millis(800))
            .await;
        if let Some(position) = click {
            eprintln!("clicking the popup at {position:?}");
            let _ = cx.update_window(popup_window.into(), |_, window, cx| {
                let modifiers = Modifiers::default();
                window.dispatch_event(
                    PlatformInput::MouseMove(MouseMoveEvent {
                        position,
                        pressed_button: None,
                        modifiers,
                    }),
                    cx,
                );
                window.dispatch_event(
                    PlatformInput::MouseDown(MouseDownEvent {
                        button: MouseButton::Left,
                        position,
                        modifiers,
                        click_count: 1,
                        first_mouse: false,
                    }),
                    cx,
                );
                window.dispatch_event(
                    PlatformInput::MouseUp(MouseUpEvent {
                        button: MouseButton::Left,
                        position,
                        modifiers,
                        click_count: 1,
                    }),
                    cx,
                );
                window.refresh();
            });
            cx.background_executor()
                .timer(Duration::from_millis(3000))
                .await;
        }
        let snapshot = snapshot.trim().to_string();
        if !snapshot.is_empty() {
            for (handle, path) in [
                (main_window, snapshot.clone()),
                (
                    popup_window,
                    format!("{}-popup.png", snapshot.trim_end_matches(".png")),
                ),
            ] {
                let _ = handle.update(cx, |_, window, _| window.refresh());
                cx.background_executor()
                    .timer(Duration::from_millis(300))
                    .await;
                let _ = handle.update(cx, |_, window, _| match window.render_to_image() {
                    Ok(image) => match image.save(&path) {
                        Ok(()) => eprintln!("wrote {path}"),
                        Err(error) => eprintln!("couldn't write {path}: {error}"),
                    },
                    Err(error) => eprintln!("couldn't draw {path}: {error}"),
                });
            }
        }
        let _ = cx.update(|cx| cx.quit());
    })
    .detach();
}

#[cfg(not(feature = "modal-demo-snapshot"))]
fn open_windows(cx: &mut App) {
    eprintln!(
        "start-menu-click needs: cargo build --bin native-modal-demo --features modal-demo-snapshot"
    );
    cx.quit();
}
