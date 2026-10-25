mod bootstrap;
mod static_files;

use std::{io::Write as _, path::PathBuf, process::Command, sync::Arc};

use axum::{
    body::Body,
    extract::State,
    http::{header, Method, Request, StatusCode},
    response::{IntoResponse, Response},
    Json, Router,
};

use crate::{
    auth::is_expected_gxserver_auth_token,
    config::{read_gxserver_config, GxserverConfig},
    ghostex_cli::rpc::{CliError, CliResult},
    paths::get_gxserver_paths,
    protocol::rpc_error,
};

const USAGE: &str = "Usage: ghostex web [--port <port>] [--dist-dir <directory>] [--no-open]\n\nStart Ghostex Web on http://127.0.0.1:4173 and open it in the default browser.\nRuns in the foreground; Ctrl+C stops only the web server.\nServes the GPUI web build (cargo xtask web-build) and requires a running gxserver.";

/// CDXC:ServerApi 2026-09-06 DECISION:
/// User: `ghostex web` must run its own separate server, not be served automatically on gxserver's ports.
/// Static files and the browser bootstrap belong to this foreground CLI process; the browser uses gxserver directly for authenticated HTTP and WebSocket APIs.
/// SEE-ALSO: apps/desktop/src/app/helpers/os_cli/process_and_constants.rs, apps/gpui-web/www/vite.config.js.
pub fn web_command(args: &[String]) -> CliResult<()> {
    let mut port = 4173_u16;
    let mut dist_dir = None;
    let mut open_browser = true;
    let mut args = args.iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "help" | "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            "--no-open" => open_browser = false,
            "--port" => {
                port = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .filter(|port| *port > 0)
                    .ok_or_else(|| {
                        CliError::Other("--port requires a number from 1 to 65535.".into())
                    })?;
            }
            "--dist-dir" => {
                dist_dir = Some(PathBuf::from(args.next().ok_or_else(|| {
                    CliError::Other("--dist-dir requires a directory.".into())
                })?));
            }
            _ => {
                return Err(CliError::Other(format!(
                    "Unknown web argument: {argument}\n\n{USAGE}"
                )))
            }
        }
    }
    let mut config = read_gxserver_config(&get_gxserver_paths(None))?;
    if let Some(dist_dir) = dist_dir {
        config.web.dist_dir = Some(dist_dir);
    }
    let dist_dir = static_files::resolve_web_dist_dir(&config);
    if !dist_dir.join("index.html").is_file() {
        return Err(CliError::Other(format!("Ghostex Web is not built at {}. Run `cargo xtask web-build` from the checkout or pass --dist-dir <directory>.", dist_dir.display())));
    }
    config.web.dist_dir = Some(dist_dir.canonicalize()?);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(serve(config, port, open_browser))
}

struct WebState {
    config: GxserverConfig,
    port: u16,
    api_url: String,
    bootstrap_secret: String,
}

/// The header that carries this run's key on `/api/webBootstrap`.
const BOOTSTRAP_SECRET_HEADER: &str = "x-ghostex-web-bootstrap";

/// A fresh random key for this `ghostex web` run (32 bytes, hex).
fn new_bootstrap_secret() -> String {
    use rand::RngCore as _;
    let mut bytes = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Writes a page that forwards the browser to `page_url`, readable only by this user, so the key
/// never appears on a command line (other accounts can read process arguments). Jupyter opens its
/// browser the same way.
fn write_browser_launcher(page_url: &str, port: u16) -> CliResult<PathBuf> {
    let path = get_gxserver_paths(None)
        .auth_dir
        .join(format!("web-open-{port}.html"));
    let _ = std::fs::remove_file(&path);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(&path)?;
    file.write_all(
        format!(
            "<!doctype html><meta charset=\"utf-8\"><meta http-equiv=\"refresh\" content=\"0;url={page_url}\"><title>Ghostex Web</title><a href=\"{page_url}\">Open Ghostex Web</a>"
        )
        .as_bytes(),
    )?;
    Ok(path)
}

/// CDXC:ServerApi 2026-10-25 WHY:
/// `/api/webBootstrap` hands out the gxserver token, and its only gate was the Origin header, which any non-browser client (another account on the computer, for one) can leave out or forge (security audit 2026-10-25). Each run now makes a random key that only the page `ghostex web` opens receives, in its address fragment through a private launcher file (or the printed link for `--no-open`), and the bootstrap refuses any request without it. The Host and Origin checks stay.
/// SEE-ALSO: apps/gpui-web/src/app/gx_store/web_transport.rs `bootstrap_secret`, apps/gpui-web/www/vite.config.js.
async fn serve(config: GxserverConfig, port: u16, open_browser: bool) -> CliResult<()> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|error| {
            CliError::Other(format!("Cannot start Ghostex Web on port {port}: {error}"))
        })?;
    let url = format!("http://127.0.0.1:{port}/");
    let api_url = format!(
        "http://{}:{}",
        config.listeners.local.host, config.listeners.local.port
    );
    let bootstrap_secret = new_bootstrap_secret();
    let page_url = format!("{url}#bootstrap={bootstrap_secret}");
    let router = Router::new()
        .fallback(handle_request)
        .with_state(Arc::new(WebState {
            config,
            port,
            api_url,
            bootstrap_secret,
        }));
    println!("Ghostex Web: {page_url}\nPress Ctrl+C to stop the web server.");
    let launcher = if open_browser {
        let launcher = write_browser_launcher(&page_url, port)?;
        browser_open_command()?
            .arg(&launcher)
            .spawn()
            .map_err(|error| CliError::Other(format!("Failed to open {url}: {error}")))?;
        Some(launcher)
    } else {
        None
    };
    let served = axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
    if let Some(launcher) = launcher {
        let _ = std::fs::remove_file(launcher);
    }
    served?;
    Ok(())
}

async fn handle_request(State(state): State<Arc<WebState>>, request: Request<Body>) -> Response {
    let headers = request.headers();
    let authority = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let allowed_host = authority == format!("127.0.0.1:{}", state.port)
        || authority == format!("localhost:{}", state.port);
    let expected_origin = format!("http://{authority}");
    let allowed_origin = headers
        .get_all(header::ORIGIN)
        .iter()
        .all(|origin| origin.to_str().ok() == Some(expected_origin.as_str()));
    if !allowed_host || !allowed_origin {
        return StatusCode::FORBIDDEN.into_response();
    }
    let path = request.uri().path();
    if path == "/api/webBootstrap" {
        let presented = headers
            .get(BOOTSTRAP_SECRET_HEADER)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if !is_expected_gxserver_auth_token(presented, &state.bootstrap_secret) {
            return (
                StatusCode::FORBIDDEN,
                Json(rpc_error(
                    "forbidden",
                    "Open Ghostex Web with the link `ghostex web` printed; it carries this run's key.",
                    None,
                )),
            )
                .into_response();
        }
        return if request.method() == Method::POST {
            bootstrap::bootstrap(headers, &state.api_url)
        } else {
            StatusCode::METHOD_NOT_ALLOWED.into_response()
        };
    }
    if path.starts_with("/api/") || path.starts_with("/ext/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let mut response = static_files::serve_web_static(&state.config, path).await;
    if request.method() == Method::HEAD {
        *response.body_mut() = Body::empty();
    }
    response
}

#[cfg(target_os = "macos")]
pub(super) fn browser_open_command() -> CliResult<Command> {
    Ok(Command::new("open"))
}

#[cfg(target_os = "linux")]
pub(super) fn browser_open_command() -> CliResult<Command> {
    Ok(Command::new("xdg-open"))
}

#[cfg(target_os = "windows")]
pub(super) fn browser_open_command() -> CliResult<Command> {
    let mut command = Command::new("rundll32.exe");
    command.arg("url.dll,FileProtocolHandler");
    Ok(command)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub(super) fn browser_open_command() -> CliResult<Command> {
    Err(CliError::Other(
        "Browser opening is unsupported on this platform. Use --no-open.".into(),
    ))
}
