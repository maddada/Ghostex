//! The sidecar's start and its loopback listener.
//!
//! gxserver starts `ghostex-collab serve`, writes one [`SidecarHello`] line to its stdin and reads
//! one [`SidecarReady`] line from its stdout. The listener takes only calls carrying the sidecar
//! token from that hello. When gxserver goes away (stdin closes) the sidecar ends too, so no
//! relay link outlives the gxserver that started it.

use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use ghostex_gx_protocol::collab_component::{
    OwnerCall, SidecarEvent, SidecarHello, SidecarReady, COLLAB_PROTOCOL_VERSION,
    SIDECAR_EVENT_PATH, SIDECAR_HEALTH_PATH, SIDECAR_OWNER_PATH,
};
use serde_json::json;
use subtle::ConstantTimeEq;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Clone)]
struct Ctx {
    token: Arc<String>,
}

fn authorized(ctx: &Ctx, headers: &HeaderMap) -> bool {
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| {
            token.len() == ctx.token.len()
                && bool::from(token.as_bytes().ct_eq(ctx.token.as_bytes()))
        })
}

async fn owner(
    State(ctx): State<Ctx>,
    headers: HeaderMap,
    Json(call): Json<OwnerCall>,
) -> impl IntoResponse {
    if !authorized(&ctx, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "unauthorized"})),
        )
            .into_response();
    }
    Json(crate::owner::handle(call).await).into_response()
}

async fn event(
    State(ctx): State<Ctx>,
    headers: HeaderMap,
    Json(event): Json<SidecarEvent>,
) -> impl IntoResponse {
    if !authorized(&ctx, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "unauthorized"})),
        )
            .into_response();
    }
    tokio::task::spawn_blocking(move || handle_event(event))
        .await
        .ok();
    Json(json!({})).into_response()
}

/// One gxserver event (blocking: it may call back into gxserver).
pub fn handle_event(event: SidecarEvent) {
    match event {
        SidecarEvent::ChatDeleted {
            project_id,
            session_id,
        } => crate::owner::end_share_of_deleted_chat(&project_id, &session_id),
        SidecarEvent::OwnerTyping {
            project_id,
            session_id,
            typing,
        } => crate::guest::owner_typing(&project_id, &session_id, typing),
        SidecarEvent::EndAll => {
            for share in crate::shares::stop_all() {
                crate::sync::chat_changed(&share.project_id, &share.session_id);
            }
            crate::notify_changed();
            crate::sync::drain();
        }
    }
}

async fn health(State(ctx): State<Ctx>, headers: HeaderMap) -> impl IntoResponse {
    if !authorized(&ctx, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(json!({"ok": true, "version": env!("CARGO_PKG_VERSION"), "protocol": COLLAB_PROTOCOL_VERSION})).into_response()
}

fn router(token: String) -> Router {
    Router::new()
        .route(SIDECAR_OWNER_PATH, post(owner))
        .route(SIDECAR_EVENT_PATH, post(event))
        .route(SIDECAR_HEALTH_PATH, get(health))
        .with_state(Ctx {
            token: Arc::new(token),
        })
}

/// Reads the hello, starts everything, prints the ready line and serves until gxserver goes away.
pub async fn serve() -> Result<(), String> {
    let mut stdin = BufReader::new(tokio::io::stdin());
    let mut line = String::new();
    stdin
        .read_line(&mut line)
        .await
        .map_err(|error| format!("couldn't read the hello: {error}"))?;
    let hello: SidecarHello =
        serde_json::from_str(line.trim()).map_err(|error| format!("unreadable hello: {error}"))?;
    if hello.protocol != COLLAB_PROTOCOL_VERSION {
        return Err(format!(
            "gxserver speaks Collab protocol {}, this component {COLLAB_PROTOCOL_VERSION}",
            hello.protocol
        ));
    }
    if hello.host_token.len() < 32 || hello.sidecar_token.len() < 32 {
        return Err("the hello's secrets are too short".to_string());
    }
    let data_dir = std::path::PathBuf::from(&hello.data_dir);
    std::fs::create_dir_all(&data_dir)
        .map_err(|error| format!("couldn't make {}: {error}", data_dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o700));
    }
    crate::set_data_dir(data_dir);
    crate::host::set(Arc::new(crate::host::HttpHost {
        base_url: hello.host_url.trim_end_matches('/').to_string(),
        token: hello.host_token.clone(),
    }));
    crate::host::set_address(crate::host::HostAddress {
        base_url: hello.host_url.trim_end_matches('/').to_string(),
        gxserver_protocol_version: hello.gxserver_protocol_version,
    });

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|error| format!("couldn't listen: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    tokio::spawn(crate::sync::run());
    tokio::spawn(crate::connector::run_forever());
    let app = router(hello.sidecar_token);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let ready = SidecarReady {
        protocol: COLLAB_PROTOCOL_VERSION,
        port,
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let mut stdout = tokio::io::stdout();
    let _ = stdout
        .write_all(format!("{}\n", serde_json::to_string(&ready).unwrap_or_default()).as_bytes())
        .await;
    let _ = stdout.flush().await;

    // Until gxserver closes our stdin (it stopped, or uninstalls Collab).
    let mut rest = String::new();
    loop {
        rest.clear();
        match stdin.read_line(&mut rest).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
    }
    Ok(())
}
