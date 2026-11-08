//! Preview of the chat header's Share popover (Ghostex Collab), drawn from the app's own view
//! (app/window/share_popover_view.rs) over a backdrop, under a stand-in of the header's Share
//! button. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`):
//! `not-installed` (the default), `install-waiting`, `downloading`, `ready`, `lifetime-menu` (the
//! chip opened on "Until I stop it"), `creating`, `created` (the link and code just made),
//! `watching` (two guests), `until-stopped` (watching, a link that never runs out),
//! `reopened` (the link hidden after a restart), `message-waiting`, `confirm-stop`, `ended`.
//! `GHOSTEX_NATIVE_MODAL_DEMO_THEME=light` draws the light look; `GHOSTEX_SHARE_DEMO_BACKDROP`
//! names a picture to put behind it (else a warm gradient) and `GHOSTEX_SHARE_DEMO_BACKDROP_BLURRED`
//! the same picture blurred, drawn under the panel as the app's window glass would. Clicks print
//! the action.
use std::rc::Rc;

use gpui::{
    App, AppContext as _, Context, IntoElement, ParentElement as _, Render, Styled as _,
    StyledImage as _, Window, div, px, rgb, svg,
};
use gpui_component::h_flex;

use super::share_popover_view::{
    self as view, Guest, Lifetime, Live, SHARE_POPOVER_WIDTH, ShareModel, Stage,
};

const NOW_MS: i64 = 1_762_600_000_000;

fn guests(waiting: bool) -> Vec<Guest> {
    vec![
        Guest {
            id: "g1".into(),
            name: "Alper".into(),
            joined_at_ms: Some(NOW_MS - 12 * 60_000),
            waiting,
        },
        Guest {
            id: "g2".into(),
            name: "Sam".into(),
            joined_at_ms: Some(NOW_MS - 3 * 60_000),
            waiting: false,
        },
    ]
}

fn live(link: bool, guests: Vec<Guest>, expires: Option<i64>) -> Live {
    let waiting = guests.iter().filter(|guest| guest.waiting).count();
    Live {
        share_id: "share_demo".into(),
        link: link
            .then(|| "https://share.ghostex.dev/s/3f9a1c2e8b7d4a10/#Kq7vX2mP9tLwR4sYzN8bQe".into()),
        code: link.then(|| "48219037".into()),
        expires_at_ms: expires,
        guests,
        waiting,
        connected: true,
        mode: 1,
    }
}

pub(crate) fn model(state: &str, light: bool) -> ShareModel {
    let in_an_hour = Some(NOW_MS + 59 * 60_000);
    let in_47 = Some(NOW_MS + 47 * 60_000);
    let stage = match state {
        "install-waiting" => Stage::InstallWaiting,
        "downloading" => Stage::Installing {
            fraction: Some(0.42),
        },
        "ready" | "lifetime-menu" => Stage::Ready,
        "creating" => Stage::Creating,
        "created" => Stage::Live(live(true, Vec::new(), in_an_hour)),
        "watching" => Stage::Live(live(true, guests(false), in_47)),
        "until-stopped" => Stage::Live(live(true, guests(false), None)),
        "reopened" => Stage::Live(live(false, guests(false), in_47)),
        "message-waiting" => Stage::Live(live(true, guests(true), in_47)),
        "confirm-stop" => Stage::ConfirmStop(live(true, guests(false), in_47)),
        "ended" => Stage::Ended,
        _ => Stage::NotInstalled { can_install: true },
    };
    let mut model = ShareModel::new(stage);
    model.now_ms = NOW_MS;
    model.light = light;
    model.glass = true;
    if state == "lifetime-menu" {
        model.lifetime_menu_open = true;
        model.lifetime = Lifetime::UntilStopped;
    }
    model
}

struct SharePopoverDemo {
    model: ShareModel,
    backdrop: Option<std::path::PathBuf>,
    /// The same picture blurred, drawn under the panel the way the app's window glass blurs what
    /// is behind the popover.
    blurred: Option<std::path::PathBuf>,
}

const WINDOW_WIDTH: f32 = SHARE_POPOVER_WIDTH + 40.0;
const WINDOW_HEIGHT: f32 = 600.0;
const POPOVER_TOP: f32 = 46.0;
const POPOVER_LEFT: f32 = WINDOW_WIDTH - 16.0 - SHARE_POPOVER_WIDTH;

impl Render for SharePopoverDemo {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let on: view::ShareHandler<Self> = Rc::new(|_, action, _, _| eprintln!("{action:?}"));
        let light = self.model.light;
        let shared = matches!(self.model.stage, Stage::Live(_) | Stage::ConfirmStop(_));
        let ink = if light { rgb(0x1a1a1c) } else { rgb(0xf2f2f2) };
        let label = match &self.model.stage {
            Stage::Live(live) | Stage::ConfirmStop(live) if !live.guests.is_empty() => {
                format!("Shared \u{b7} {}", live.guests.len())
            }
            _ if shared => "Shared".to_string(),
            _ => "Share".to_string(),
        };
        let backdrop = match &self.backdrop {
            Some(path) => gpui::img(path.clone())
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .object_fit(gpui::ObjectFit::Cover)
                .into_any_element(),
            None => div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(gpui::linear_gradient(
                    160.0,
                    gpui::linear_color_stop(if light { rgb(0xd8cfc4) } else { rgb(0x3a2a22) }, 0.0),
                    gpui::linear_color_stop(if light { rgb(0x9fb3c2) } else { rgb(0x16222b) }, 1.0),
                ))
                .into_any_element(),
        };
        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .child(backdrop)
            .child(
                h_flex()
                    .absolute()
                    .top(px(14.0))
                    .right(px(24.0))
                    .h(px(26.0))
                    .px(px(9.0))
                    .gap(px(5.0))
                    .items_center()
                    .rounded(px(6.0))
                    .bg(gpui::Rgba {
                        r: 1.0,
                        g: 1.0,
                        b: 1.0,
                        a: if light { 0.45 } else { 0.14 },
                    })
                    .text_size(px(12.0))
                    .text_color(ink)
                    .child(
                        svg()
                            .path("titlebar/users.svg")
                            .size(px(14.0))
                            .text_color(ink),
                    )
                    .when(shared, |this| {
                        this.child(div().size(px(6.0)).rounded_full().bg(rgb(0x4ade80)))
                    })
                    .child(label),
            )
            .child(
                div()
                    .absolute()
                    .top(px(POPOVER_TOP))
                    .left(px(POPOVER_LEFT))
                    .w(px(SHARE_POPOVER_WIDTH))
                    .rounded(px(10.0))
                    .overflow_hidden()
                    .when_some(self.blurred.clone(), |this, path| {
                        this.child(
                            gpui::img(path)
                                .absolute()
                                .top(px(-POPOVER_TOP))
                                .left(px(-POPOVER_LEFT))
                                .w(px(WINDOW_WIDTH))
                                .h(px(WINDOW_HEIGHT))
                                .object_fit(gpui::ObjectFit::Cover),
                        )
                    })
                    .child(view::render_share_popover(&self.model, &on, cx)),
            )
    }
}

use gpui::prelude::FluentBuilder as _;

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let light = super::env("GHOSTEX_NATIVE_MODAL_DEMO_THEME") == "light";
    let state = if demo.state.is_empty() {
        "not-installed".to_string()
    } else {
        demo.state.clone()
    };
    let backdrop = std::env::var_os("GHOSTEX_SHARE_DEMO_BACKDROP")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_file());
    let blurred = std::env::var_os("GHOSTEX_SHARE_DEMO_BACKDROP_BLURRED")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_file());
    let model = model(&state, light);
    super::open_modal_window(
        WINDOW_WIDTH,
        WINDOW_HEIGHT,
        move |_, cx| {
            cx.new(|_| SharePopoverDemo {
                model,
                backdrop,
                blurred,
            })
        },
        cx,
    );
}
