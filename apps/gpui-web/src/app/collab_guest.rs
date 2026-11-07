//! Ghostex Collab's guest mode: the page `share.ghostex.dev/s#<key>` opened, and
//! `www/src/collab/guest.js` connected the guest to the owner's computer and put
//! `globalThis.ghostexCollabGuest` in place before the app started. The app then shows that one
//! chat and nothing else: no sidebar, no other session, no Settings, no terminal. Its gxserver
//! calls go to the guest endpoint, which the page carries through the end-to-end lock, where the
//! owner's gxserver answers them with the share's token (server/src/server/collab_connector.rs).
//!
//! CDXC:Collab 2026-11-07 DECISION:
//! User (Q1612, Q1617): the guest's page is "the GPUI web build in a guest mode that shows only that one chat", and "a guest can never answer permission prompts or see anything but that chat".

use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div,
    prelude::*, px, rgb,
};
use gpui_component::h_flex;
use serde_json::{Value, json};
use wasm_bindgen::JsValue;

use crate::GhostexGpuiApp;
use crate::app::helpers::*;
use crate::{WORKAREA_HEADER_EDGE_PADDING, WORKAREA_HEADER_HEIGHT};

#[derive(Clone, Debug)]
pub(crate) struct CollabGuest {
    pub(crate) base_url: String,
    pub(crate) viewer_id: String,
    pub(crate) name: String,
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) title: String,
    pub(crate) owner: String,
}

thread_local! {
    static GUEST: Option<CollabGuest> = read_guest();
}

fn read_guest() -> Option<CollabGuest> {
    let value =
        js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("ghostexCollabGuest")).ok()?;
    if value.is_undefined() || value.is_null() {
        return None;
    }
    let field = |key: &str| {
        js_sys::Reflect::get(&value, &JsValue::from_str(key))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default()
    };
    Some(CollabGuest {
        base_url: field("baseUrl"),
        viewer_id: field("viewerId"),
        name: field("name"),
        project_id: field("projectId"),
        session_id: field("sessionId"),
        title: field("title"),
        owner: field("owner"),
    })
    .filter(|guest| !guest.base_url.is_empty() && !guest.session_id.is_empty())
}

/// The guest this page belongs to, or `None` on an ordinary Ghostex Web page.
pub(crate) fn guest() -> Option<CollabGuest> {
    GUEST.with(Clone::clone)
}

/// The page id in a Ghostex page's address (`…/visual/<32 hex>`), whatever host it names.
fn visual_page_id(url: &str) -> Option<&str> {
    let rest = &url[url.find("/visual/")? + "/visual/".len()..];
    let id = rest.get(..32)?;
    let ends = rest[32..]
        .chars()
        .next()
        .is_none_or(|next| matches!(next, '?' | '#' | '/'));
    (ends && id.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(id)
}

/// Hands `request` to the guest's page viewer (www/src/collab/page.js).
fn open_in_page_viewer(request: Value) -> bool {
    let Ok(open) = js_sys::Reflect::get(
        &js_sys::global(),
        &JsValue::from_str("ghostexCollabOpenPage"),
    ) else {
        return false;
    };
    let Some(open) = wasm_bindgen::JsCast::dyn_ref::<js_sys::Function>(&open) else {
        return false;
    };
    let Ok(request) = js_sys::JSON::parse(&request.to_string()) else {
        return false;
    };
    open.call1(&JsValue::NULL, &request).is_ok()
}

/// On a guest's page, a Ghostex page's address (a page card's Open) opens in the guest's page
/// viewer, which reads it through the share's tunnel: the address names the owner's computer,
/// which the guest's browser cannot reach. `false` when this is not a guest's page or not a page.
pub(crate) fn open_page_for_guest(url: &str) -> bool {
    if guest().is_none() {
        return false;
    }
    let Some(page_id) = visual_page_id(url) else {
        return false;
    };
    open_in_page_viewer(json!({ "pageId": page_id }))
}

/// On a guest's page, an HTML file the chat links to opens in the guest's page viewer (gxserver
/// checks that the shared chat shows it). `false` for any other file.
pub(crate) fn open_file_for_guest(path: &str) -> bool {
    let is_html = std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "html" | "htm"));
    if guest().is_none() || !is_html {
        return false;
    }
    let title = std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    open_in_page_viewer(json!({ "path": path, "title": title }))
}

/// The share's chat, as the guest's store would key it.
fn guest_session(guest: &CollabGuest) -> ghostex_gx_core::SessionKey {
    ghostex_gx_core::SessionKey {
        machine: ghostex_gx_core::MachineId::Local,
        project_id: guest.project_id.clone(),
        session_id: guest.session_id.clone(),
    }
}

impl GhostexGpuiApp {
    /// In place of `gx_store_start`: no store, no sidebar, only the shared chat.
    pub(crate) fn collab_guest_start(&mut self, cx: &mut Context<Self>) {
        let Some(guest) = guest() else {
            return;
        };
        cx.spawn(async move |app, cx| {
            let Ok(endpoint) = crate::app::gx_store::web_transport::bootstrap().await else {
                return;
            };
            crate::app::gx_chat::set_endpoint(
                crate::app::gx_chat::LOCAL_MACHINE_ID,
                &endpoint.base_url,
                &endpoint.auth_token,
            );
            let session = guest_session(&guest);
            let Ok(view) = app.update(cx, |app, cx| {
                app.open_session = Some(session.clone());
                let view = app.ensure_native_chat(&session, cx);
                cx.notify();
                view
            }) else {
                return;
            };
            // The chat's core starts a moment after its view: tell it once it can hear.
            for _ in 0..100 {
                let told = view
                    .update(cx, |view, cx| {
                        if view.runtime.is_none() {
                            return false;
                        }
                        view.invoke(
                            json!({"type": "setCollabGuest", "viewerId": guest.viewer_id, "name": guest.name}),
                            cx,
                        );
                        true
                    })
                    ;
                if told {
                    break;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
            }
        })
        .detach();
    }

    fn collab_guest_state(&self, cx: &Context<Self>) -> Value {
        self.open_session
            .as_ref()
            .and_then(|session| self.native_chats.get(session))
            .map(|(_, view)| view.read(cx).snapshot["collab"].clone())
            .unwrap_or(Value::Null)
    }

    /// The guest page: a slim header (the chat's title, who shared it, who is watching) over
    /// the chat.
    pub(crate) fn render_collab_guest_shell(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let guest = guest();
        let text = titlebar_active_text_color();
        let collab = self.collab_guest_state(cx);
        let owner = collab["owner"]
            .as_str()
            .map(str::to_string)
            .or_else(|| guest.as_ref().map(|guest| guest.owner.clone()))
            .unwrap_or_default();
        let title = guest
            .as_ref()
            .map(|guest| guest.title.clone())
            .unwrap_or_default();
        let live = collab["role"] == "guest";
        let watching = collab["viewers"]
            .as_array()
            .map(|names| {
                names
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let surface = self
            .open_session
            .as_ref()
            .and_then(|session| self.native_chats.get(session))
            .map(|(_, view)| view.clone().into_any_element());
        let header = h_flex()
            .id("collab-guest-header")
            .h(px(WORKAREA_HEADER_HEIGHT))
            .w_full()
            .flex_none()
            .px(px(WORKAREA_HEADER_EDGE_PADDING + 4.0))
            .gap(px(10.0))
            .items_center()
            .border_b_1()
            .border_color(titlebar_popup_menu_border_color())
            .text_size(px(12.5))
            .child(
                h_flex()
                    .min_w_0()
                    .flex_1()
                    .gap(px(6.0))
                    .overflow_hidden()
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(text)
                            .child(SharedString::from(title)),
                    )
                    .when(!owner.is_empty(), |row| {
                        row.child(
                            div()
                                .flex_none()
                                .text_color(text.opacity(0.55))
                                .child(SharedString::from(format!("\u{b7} Shared by {owner}"))),
                        )
                    }),
            )
            .child(
                h_flex()
                    .flex_none()
                    .gap(px(6.0))
                    .items_center()
                    .text_color(text.opacity(0.7))
                    .child(div().size(px(7.0)).rounded_full().bg(if live {
                        rgb(0x4ade80)
                    } else {
                        rgb(0x8a8a8a)
                    }))
                    .child(if live { "Live" } else { "Connecting" })
                    .when(!watching.is_empty(), |row| {
                        row.child(
                            div()
                                .max_w(px(320.0))
                                .truncate()
                                .text_color(text.opacity(0.55))
                                .child(SharedString::from(watching)),
                        )
                    }),
            );
        div()
            .id("ghostex-collab-guest")
            .size_full()
            .flex()
            .flex_col()
            .bg(titlebar_background())
            .child(header)
            .child(
                div().flex_1().min_h_0().flex().child(match surface {
                    Some(surface) => div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .overflow_hidden()
                        .child(surface),
                    None => div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(0x8a8a8a))
                        .child("Opening the chat\u{2026}"),
                }),
            )
            .children(self.render_web_toasts())
            .into_any_element()
    }
}
