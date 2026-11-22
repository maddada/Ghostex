//! `GET /connectors/composio/oauth`: where Composio sends the user's browser back after they sign
//! in (Open Dot's `api/composio/oauth/route.ts`). It finishes the sign-in of the workspace whose
//! `state` came back and shows a small page; Settings updates through the `connectorsChanged`
//! event. The browser has no gxserver token, so the request is trusted only through the OAuth
//! `state` a sign-in issued (and only for 10 minutes): without it nothing changes and no event
//! goes out.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, HeaderValue, Response, StatusCode};

use crate::server::{AppState, RoutedResponse};

use super::super::workspace::connector_workspaces;
use super::account::{cancel_pending, workspace_for_state, STALE_LINK};
use super::signin::finish_sign_in;

/// The path Composio redirects to, on gxserver's local listener.
pub(crate) const OAUTH_REDIRECT_PATH: &str = "/connectors/composio/oauth";

pub(crate) fn redirect_uri(redirect_base: &str) -> String {
    format!(
        "{}{OAUTH_REDIRECT_PATH}",
        redirect_base.trim_end_matches('/')
    )
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn page(error: Option<&str>) -> String {
    let title = if error.is_some() {
        "Couldn't connect Composio"
    } else {
        "Composio is connected"
    };
    let body = match error {
        Some(error) => escape(error),
        None => "Composio is connected. You can close this tab and go back to Ghostex.".to_string(),
    };
    format!(
        r##"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} · Ghostex</title>
<style>
  :root {{ color-scheme: light dark; --bg: #f6f6f6; --card: #fff; --fg: #0a0a0a; --muted: #0a0a0a99; --line: #0a0a0a14; }}
  @media (prefers-color-scheme: dark) {{ :root {{ --bg: #0e0e0e; --card: #171717; --fg: #f4f4f4; --muted: #f4f4f499; --line: #ffffff14; }} }}
  body {{ margin: 0; min-height: 100vh; display: grid; place-items: center; background: var(--bg); color: var(--fg);
    font: 15px/1.5 -apple-system, system-ui, sans-serif; padding: 16px; box-sizing: border-box; }}
  .card {{ max-width: 420px; width: 100%; background: var(--card); border: 1px solid var(--line); border-radius: 16px; padding: 32px; box-sizing: border-box; }}
  .dots {{ display: flex; margin-bottom: 20px; }} .dots span {{ width: 16px; height: 16px; border-radius: 50%; margin-right: -4px; box-shadow: 0 0 0 3px var(--card); }}
  h1 {{ font-size: 20px; font-weight: 600; margin: 0 0 8px; }} p {{ margin: 0; color: var(--muted); }}
</style></head><body><div class="card">
<div class="dots"><span style="background:#0a0a0a"></span><span style="background:#51a2ff"></span><span style="background:#c8f169"></span></div>
<h1>{title}</h1><p>{body}</p></div></body></html>"##
    )
}

fn html_response(status: StatusCode, html: String) -> RoutedResponse {
    let mut response = Response::new(Body::from(html));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    RoutedResponse {
        endpoint_path: None,
        response,
    }
}

/// Handles the redirect: `code` + `state`, or `error` / `error_description` when the user
/// declined.
pub(crate) async fn serve_oauth_redirect(
    state: Arc<AppState>,
    query: Option<String>,
) -> RoutedResponse {
    let mut code = None;
    let mut oauth_state = None;
    let mut error = None;
    let mut error_description = None;
    for (key, value) in url::form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => oauth_state = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            "error_description" => error_description = Some(value.into_owned()),
            _ => {}
        }
    }
    let given_state = oauth_state.unwrap_or_default();
    let check_paths = state.paths.clone();
    let check_state = given_state.clone();
    let workspace = tokio::task::spawn_blocking(move || {
        let workspaces: Vec<String> = connector_workspaces(&check_paths)
            .map(|workspaces| {
                workspaces
                    .into_iter()
                    .map(|workspace| workspace.id)
                    .collect()
            })
            .unwrap_or_default();
        workspace_for_state(&check_paths, &workspaces, &check_state)
    })
    .await
    .ok()
    .flatten();
    let Some(workspace) = workspace else {
        return html_response(StatusCode::BAD_REQUEST, page(Some(STALE_LINK)));
    };
    let mut failure = error_description.or(error);
    if failure.is_some() {
        let paths = state.paths.clone();
        let declined_workspace = workspace.clone();
        let _ = tokio::task::spawn_blocking(move || {
            cancel_pending(&paths, &declined_workspace, &given_state)
        })
        .await;
    } else {
        match code {
            Some(code) => {
                let paths = state.paths.clone();
                let finished_workspace = workspace.clone();
                let finished = tokio::task::spawn_blocking(move || {
                    finish_sign_in(&paths, &finished_workspace, &code, &given_state)
                })
                .await
                .unwrap_or_else(|_| {
                    Err("Composio sign-in didn't complete. Try again.".to_string())
                });
                if let Err(message) = finished {
                    failure = Some(message);
                }
            }
            None => failure = Some("Composio didn't send a sign-in code. Try again.".to_string()),
        }
    }
    super::super::broadcast_connectors_changed(&state, &workspace);
    match failure {
        Some(message) => html_response(StatusCode::BAD_REQUEST, page(Some(&message))),
        None => html_response(StatusCode::OK, page(None)),
    }
}
