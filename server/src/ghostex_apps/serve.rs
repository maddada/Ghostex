//! `GET /app/<id>` (the app's newest version) and `GET /app/<id>/icon`, answered before the auth
//! gate like a Live page, under the same sandbox.

use std::sync::Arc;

use axum::{
    body::Body,
    http::{header, HeaderValue, Response, StatusCode},
    response::IntoResponse,
};

use crate::server::{AppState, RoutedResponse};

use super::{
    feature::apps_on,
    store::{is_app_id, read_current_html, read_icon},
};

pub(crate) const APP_ROUTE_PREFIX: &str = "/app/";
const ICON_SUFFIX: &str = "/icon";

/// What `window.ghostex.app` is in an app: its own data, sending to its chat, and asking whether a
/// power is allowed. Every call is a POST to the app's own `/bridge` route with the launch token
/// Ghostex put in the URL fragment when it opened the app (tokens.rs), in the `X-Ghostex-App-Token`
/// header so gxserver checks it before it reads the call; without one (the app opened
/// from a plain link, an old phone, a browser) `available` is false and every call is refused with
/// a reason the app can show.
///
/// CDXC:GhostexApps 2026-11-25 SEE-ALSO:
/// The calls are answered by server/src/ghostex_apps/bridge.rs and described to agents in server/src/ghostex_cli/ghostex_app_guide.md. Change them together.
const APP_API: &str = r#"<script>(function(){var tok="",base=location.pathname.replace(/\/+$/,"");try{var m=/[#&]gx-app-token=(gxa_[0-9a-f]{32})/.exec(location.hash);if(m)tok=m[1];}catch(e){}
function call(op,args){return new Promise(function(resolve,reject){if(!tok){var n=new Error("Open this app from Ghostex to use ghostex.app.");n.permission=null;reject(n);return;}var b={op:op};for(var k in args)b[k]=args[k];
fetch(base+"/bridge",{method:"POST",headers:{"X-Ghostex-App-Token":tok},body:JSON.stringify(b)}).then(function(r){return r.json().catch(function(){return{};}).then(function(j){if(r.ok&&j.ok){resolve(j);return;}var e=new Error(j.error||"Ghostex did not answer.");e.permission=j.permission||null;reject(e);});},function(){reject(new Error("Could not reach Ghostex."));});});}
var app={available:!!tok,
data:{get:function(k){return call("data.get",{key:String(k)}).then(function(j){return j.value;});},
set:function(k,v){return call("data.set",{key:String(k),value:v===undefined?null:v}).then(function(){});},
remove:function(k){return call("data.remove",{key:String(k)}).then(function(){});},
keys:function(){return call("data.keys",{}).then(function(j){return j.keys;});},
all:function(){return call("data.all",{}).then(function(j){return j.data;});}},
send:function(t){return call("send",{text:String(t)}).then(function(){});},
permission:function(p){return call("permission",{power:String(p)}).then(function(j){return j.state;});}};
var g=window.ghostex||{};g.app=app;window.ghostex=g;})();</script>"#;

/// Whether `GET <path>` is an app or its icon: `/app/<id>` or `/app/<id>/icon`.
pub(crate) fn is_app_route(path: &str) -> bool {
    app_of(path).is_some()
}

fn app_of(path: &str) -> Option<(String, bool)> {
    let rest = path.strip_prefix(APP_ROUTE_PREFIX)?;
    let (id, icon) = match rest.strip_suffix(ICON_SUFFIX) {
        Some(id) => (id, true),
        None => (rest.trim_end_matches('/'), false),
    };
    is_app_id(id).then(|| (id.to_string(), icon))
}

/// The app's content policy: a Live page's, with its fetches to gxserver narrowed to this app's
/// own routes (`/app/<id>/`), so it reaches its bridge and nothing else on gxserver.
fn served_policy(id: &str, host: Option<&str>) -> String {
    let policy = crate::visual_pages::PAGE_CONTENT_SECURITY_POLICY;
    let host = host.filter(|host| {
        !host.is_empty()
            && host
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b".-:[]".contains(&byte))
    });
    match host {
        Some(host) => policy.replace(
            "connect-src https: 'self'",
            &format!("connect-src https: http://{host}{APP_ROUTE_PREFIX}{id}/"),
        ),
        None => policy.replace("connect-src https: 'self'", "connect-src https:"),
    }
}

/// `GET /app/<id>` and `GET /app/<id>/icon`.
///
/// CDXC:GhostexApps 2026-11-25 WHY:
/// An app opens like a Live page (user: "open like we open live pages"), so it is served the same way: before the auth gate, since the window that shows it holds no gxserver token, under the response header's `sandbox` (an opaque origin with no cookies, no storage and no same-origin calls into gxserver) and the page content policy. Its powers are not in the page: they are the bridge's, behind a launch token (tokens.rs). While Ghostex Apps is off every app answers 404.
pub(crate) async fn serve(
    state: Arc<AppState>,
    path: String,
    host: Option<String>,
) -> RoutedResponse {
    let Some((id, icon)) = app_of(&path) else {
        return status(StatusCode::NOT_FOUND);
    };
    if !apps_on(&state.paths) {
        return status(StatusCode::NOT_FOUND);
    }
    let state_dir = state.paths.root_dir.clone();
    if icon {
        let found = tokio::task::spawn_blocking(move || read_icon(&state_dir, &id))
            .await
            .ok()
            .flatten();
        let Some(bytes) = found else {
            return status(StatusCode::NOT_FOUND);
        };
        return respond(bytes, "image/png", "sandbox; default-src 'none'");
    }
    let policy = served_policy(&id, host.as_deref());
    let header_policy = format!("sandbox allow-scripts allow-forms allow-popups; {policy}");
    let page = tokio::task::spawn_blocking(move || {
        read_current_html(&state_dir, &id).map(|html| {
            crate::visual_pages::with_app_block(
                String::from_utf8_lossy(&html).into_owned(),
                &policy,
                APP_API,
            )
        })
    })
    .await
    .ok()
    .flatten();
    let Some(page) = page else {
        return status(StatusCode::NOT_FOUND);
    };
    respond(
        page.into_bytes(),
        "text/html; charset=utf-8",
        &header_policy,
    )
}

fn respond(bytes: Vec<u8>, content_type: &str, policy: &str) -> RoutedResponse {
    let mut response = RoutedResponse {
        endpoint_path: None,
        response: Response::new(Body::from(bytes)),
    };
    for (name, value) in [
        (header::CONTENT_TYPE, content_type),
        (header::CONTENT_SECURITY_POLICY, policy),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::CACHE_CONTROL, "no-store"),
    ] {
        if let Ok(value) = HeaderValue::from_str(value) {
            response.response.headers_mut().insert(name, value);
        }
    }
    response
}

fn status(code: StatusCode) -> RoutedResponse {
    RoutedResponse {
        endpoint_path: None,
        response: code.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_and_policy_name_only_the_app() {
        let id = "0123456789abcdef0123456789abcdef";
        assert_eq!(app_of(&format!("/app/{id}")), Some((id.to_string(), false)));
        assert_eq!(
            app_of(&format!("/app/{id}/icon")),
            Some((id.to_string(), true))
        );
        assert_eq!(app_of(&format!("/app/{id}/bridge")), None);
        assert_eq!(app_of("/app/../visual"), None);
        let policy = served_policy(id, Some("127.0.0.1:4000"));
        assert!(policy.contains(&format!(
            "connect-src https: http://127.0.0.1:4000/app/{id}/;"
        )));
        assert!(!policy.contains("'self'"));
        assert!(served_policy(id, Some("evil\" onload=x")).contains("connect-src https:;"));
    }
}
