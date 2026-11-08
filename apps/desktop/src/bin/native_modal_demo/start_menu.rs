//! Preview of the header's Start (Quick Actions) menu, drawn by the app's own rows file
//! (app/titlebar/actions_menu_rows.rs): the project's Actions, then the Global Actions under
//! "Global". `GHOSTEX_NATIVE_MODAL_DEMO_STATE=no-global` shows a project with no Global Actions,
//! `global-only` one with only Global Actions. Clicks print what the app would run.
use gpui::{
    App, AppContext as _, Context, Entity, Focusable as _, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Window, div, px, rgb,
};
use gpui_component::Side;
use gpui_component::menu::{PopupMenu, PopupMenuAppearance};

use crate::app::actions::*;
use crate::app::helpers::*;

#[path = "../../app/titlebar/actions_menu_rows.rs"]
mod rows;

fn action(id: &str, name: &str, icon_path: &'static str, preview: &str) -> GpuiTitlebarAction {
    GpuiTitlebarAction {
        command_id: id.to_string(),
        name: name.to_string(),
        icon_path,
        preview: preview.to_string(),
    }
}

fn project_actions(state: &str) -> Vec<GpuiTitlebarAction> {
    if state == "global-only" {
        return Vec::new();
    }
    vec![
        action("dev", "Dev", "titlebar/player-play.svg", "bun run dev"),
        action("build", "Build", "titlebar/tools.svg", "bun run build"),
        action("test", "Test", "titlebar/flask.svg", "bun test"),
    ]
}

fn global_actions(state: &str) -> Vec<GpuiTitlebarAction> {
    if state == "no-global" {
        return Vec::new();
    }
    vec![
        action("lazygit", "Lazygit", "titlebar/git-branch.svg", "lazygit"),
        action("htop", "Activity", "titlebar/bolt.svg", "htop"),
        action(
            "dashboard",
            "Team dashboard",
            "titlebar/world.svg",
            "https://grafana.example.com/d/team",
        ),
    ]
}

struct StartMenuDemo {
    menu: Entity<PopupMenu>,
}

impl StartMenuDemo {
    fn new(state: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let actions = project_actions(&state);
        let globals = global_actions(&state);
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
        Self { menu }
    }
}

impl Render for StartMenuDemo {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x3a3c42))
            .p(px(10.0))
            .on_action(|action: &RunGpuiTitlebarAction, _, _| {
                eprintln!("run project action #{}", action.action_index)
            })
            .on_action(|action: &RunGpuiTitlebarGlobalAction, _, _| {
                eprintln!(
                    "run global action {} (runSidebarCommand scope=global)",
                    action.command_id
                )
            })
            .on_action(|_: &ConfigureGpuiTitlebarActions, _, _| {
                eprintln!("open Settings > Actions")
            })
            .child(
                div()
                    .h(px(30.0))
                    .text_size(px(13.0))
                    .text_color(titlebar_active_text_color())
                    .child("Start ▾"),
            )
            .child(div().w(px(282.0)).child(self.menu.clone()))
    }
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let state = demo.state.clone();
    super::open_modal_window(
        320.0,
        470.0,
        move |window, cx| cx.new(|cx| StartMenuDemo::new(state, window, cx)),
        cx,
    );
}
