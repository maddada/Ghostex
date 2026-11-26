//! The remote screen window: one computer's screen, and nothing else, in a window of its own.
//!
//! CDXC:RemoteScreen 2026-11-27 DECISION: the user chose that a remote screen opens in its own
//! window (Q5111), and for the spike the window holds only the screen with the window's normal
//! title bar and close: any toolbar, control or menu waits for a Live page the user approves.
//! The app starts the stream on the computer's gxserver (`/api/remoteScreenStart`, this computer's
//! or a connected one's through its tunnel), shows the viewer address it answers in the system web
//! view (web_view.rs), and closing the window ends the stream (`/api/remoteScreenStop`). It is
//! opened by `ghostex screen open`; there is no menu item yet.
use std::time::Duration;

use gpui::{
    AppContext as _, Bounds, Context, IntoElement, ParentElement as _, Render, Styled as _,
    TitlebarOptions, Window, WindowBounds, WindowOptions, div, point, px, rgb, size,
};
use serde_json::{Value, json};

use super::web_view::RemoteScreenWebView;
use crate::GhostexGpuiApp;
use crate::app::helpers::{
    gpui_local_gxserver_api_port, gpui_remote_gxserver_post_typed_operation,
    gxserver_post_typed_operation, parse_gpui_gxserver_rpc_result,
};
use crate::app::model::GpuiRemoteGxserverRequestTarget;

/// Starting waits up to 15 s on the computer for the helper's first answer.
const START_TIMEOUT: Duration = Duration::from_secs(30);
const STOP_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_WIDTH: f32 = 1280.0;
const DEFAULT_HEIGHT: f32 = 760.0;

/// Whose gxserver serves the screen.
#[derive(Clone)]
enum ScreenServer {
    ThisComputer,
    Connected(GpuiRemoteGxserverRequestTarget),
    /// A viewer address given as it is: nothing to start or stop.
    Given,
}

impl ScreenServer {
    fn post(&self, path: &str, params: &Value, timeout: Duration) -> Result<Value, String> {
        let (status, body) = match self {
            ScreenServer::ThisComputer => gxserver_post_typed_operation(path, params, timeout)?,
            ScreenServer::Connected(target) => {
                gpui_remote_gxserver_post_typed_operation(target, path, params, timeout)?
            }
            ScreenServer::Given => return Err("nothing to ask".to_string()),
        };
        if status == 404 {
            return Err("That computer's Ghostex has no remote screen.".to_string());
        }
        parse_gpui_gxserver_rpc_result(&body).map_err(|error| {
            serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|value| {
                    value
                        .get("message")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or(error)
        })
    }

    /// The loopback origin that reaches this server's gxserver from here.
    fn origin(&self) -> Option<String> {
        match self {
            ScreenServer::ThisComputer => Some(format!(
                "http://127.0.0.1:{}",
                gpui_local_gxserver_api_port()
            )),
            ScreenServer::Connected(target) => {
                Some(format!("http://127.0.0.1:{}", target.local_port))
            }
            ScreenServer::Given => None,
        }
    }

    /// Starts (or joins) the stream at the computer's full size and answers the viewer address.
    fn start(&self) -> Result<String, String> {
        let result = self.post("/api/remoteScreenStart", &json!({}), START_TIMEOUT)?;
        let path = result
            .get("path")
            .and_then(Value::as_str)
            .filter(|path| path.starts_with("/api/remoteScreen/"))
            .ok_or_else(|| "The computer sent an unreadable screen address.".to_string())?;
        Ok(format!("{}{path}", self.origin().unwrap_or_default()))
    }

    fn stop(&self) {
        if !matches!(self, ScreenServer::Given) {
            let _ = self.post("/api/remoteScreenStop", &json!({}), STOP_TIMEOUT);
        }
    }
}

struct RemoteScreenWindowRoot {
    web_view: Option<RemoteScreenWebView>,
    error: Option<String>,
}

impl Render for RemoteScreenWindowRoot {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(web_view) = &self.web_view {
            web_view.place(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: window.viewport_size(),
            });
        }
        let root = div().size_full().bg(rgb(0x000000));
        match &self.error {
            Some(error) => root
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(0xd0d0d0))
                .child(error.clone()),
            None => root,
        }
    }
}

impl GhostexGpuiApp {
    /// `openRemoteScreen`: starts the stream on that computer (or takes `viewer_url` as it is) and
    /// opens the window once there is an address to show. Errors before that come back to the CLI.
    pub(crate) fn open_remote_screen_window(
        &mut self,
        machine_id: Option<String>,
        viewer_url: Option<String>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !RemoteScreenWebView::SUPPORTED {
            return Err(
                "The remote screen window is not available on this platform yet.".to_string(),
            );
        }
        let (server, title) = match (&viewer_url, &machine_id) {
            (Some(_), _) => (ScreenServer::Given, "Remote screen".to_string()),
            (None, Some(machine_id)) => {
                let target = self
                    .gpui_remote_gxserver_request_target(machine_id)
                    .ok_or_else(|| "Connect to that computer first.".to_string())?;
                (
                    ScreenServer::Connected(target),
                    format!(
                        "{} screen",
                        crate::app::remote_conn::app_update::remote_machine_name(machine_id)
                    ),
                )
            }
            (None, None) => (
                ScreenServer::ThisComputer,
                "This computer's screen".to_string(),
            ),
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |_this, cx| {
            let starter = server.clone();
            let address = match viewer_url {
                Some(url) => Ok(url),
                None => background.spawn(async move { starter.start() }).await,
            };
            let _ = cx.update(|cx| open_window(title, server, address, cx));
        })
        .detach();
        Ok(())
    }
}

fn open_window(
    title: String,
    server: ScreenServer,
    address: Result<String, String>,
    cx: &mut gpui::App,
) {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(DEFAULT_WIDTH), px(DEFAULT_HEIGHT)),
            cx,
        ))),
        titlebar: Some(TitlebarOptions {
            title: Some(title.clone().into()),
            appears_transparent: false,
            traffic_light_position: None,
        }),
        window_min_size: Some(size(px(480.0), px(300.0))),
        app_id: crate::gpui_platform_window_app_id(),
        icon: crate::gpui_platform_window_icon(),
        focus: true,
        show: true,
        ..Default::default()
    };
    let opened = cx.open_window(options, move |window, cx| {
        window.set_window_title(&title);
        let (web_view, error) = match &address {
            Ok(url) => match RemoteScreenWebView::new(window, url) {
                Ok(web_view) => (Some(web_view), None),
                Err(error) => (
                    None,
                    Some(format!("The screen could not be shown: {error}")),
                ),
            },
            Err(error) => (None, Some(error.clone())),
        };
        let streaming = web_view.is_some() || address.is_ok();
        // The window's own close ends the stream on that computer, off the main thread.
        window.on_window_should_close(cx, move |_, cx| {
            if streaming {
                let server = server.clone();
                cx.background_executor()
                    .spawn(async move { server.stop() })
                    .detach();
            }
            true
        });
        cx.new(|_| RemoteScreenWindowRoot { web_view, error })
    });
    if let Err(error) = opened {
        log::warn!("remote screen window did not open: {error}");
    }
}
