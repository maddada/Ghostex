//! Bounded, on-demand discovery and page metadata for the native remote sites menu.
use crate::app::helpers::*;
use crate::*;
use gpui::http_client::Url;
use scraper::{Html, Selector};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{
    io::Read,
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

#[derive(Clone)]
pub(crate) struct RemoteBrowserSite {
    pub(crate) port: u16,
    pub(crate) url: String,
    pub(crate) title: Option<String>,
    pub(crate) process: Option<String>,
    pub(crate) favicon: Option<BrowserFaviconImage>,
    pub(crate) status: Option<u16>,
    pub(crate) can_open: bool,
    pub(crate) detail: String,
    pub(crate) checked_at: SystemTime,
}

impl RemoteBrowserSite {
    pub(crate) fn label(&self) -> String {
        self.title
            .clone()
            .or_else(|| self.process.clone())
            .unwrap_or_else(|| format!("Service on port {}", self.port))
    }
    pub(crate) fn status_label(&self) -> &'static str {
        match self.status {
            Some(200..=299) => "Responding",
            Some(300..=399) => "Redirect",
            Some(401 | 403) => "Login required",
            Some(500..=599) => "Server error",
            Some(_) => "HTTP error",
            None if self.can_open => "Certificate issue",
            None => "No web response",
        }
    }
    pub(crate) fn status_color(&self) -> u32 {
        match self.status {
            Some(200..=299) => 0x7acb9d,
            Some(500..=599) => 0xe28b88,
            Some(_) => 0xd5ae6b,
            None if self.can_open => 0xd5ae6b,
            None => 0xa2a2a2,
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn discover_remote_browser_sites(
    config: &GpuiRemoteMachineConfig,
    target: &GpuiRemoteExecutionTarget,
    tunnel: Arc<RemoteBrowserTunnel>,
    canceled: Arc<AtomicBool>,
    progress: futures::channel::mpsc::UnboundedSender<RemoteBrowserSite>,
) -> Result<Vec<RemoteBrowserSite>, String> {
    let result = gpui_run_remote_ssh_in_execution_target(
        config,
        target,
        GPUI_REMOTE_LISTENING_PORTS_COMMAND,
        Duration::from_secs(12),
    );
    if result.exit_code != 0 {
        return Err(
            "Could not list this computer's listening ports. Reconnect the machine and recheck."
                .into(),
        );
    }
    let ports = gpui_parse_remote_listening_ports(&result.stdout);
    // Keep machines responsive even when several listeners never speak HTTP.
    let mut sites = Vec::new();
    for batch in ports.chunks(6) {
        if canceled.load(Ordering::Relaxed) || !tunnel.is_alive() {
            break;
        }
        let mut results = std::thread::scope(|scope| {
            let jobs: Vec<_> = batch
                .iter()
                .map(|port| {
                    let tunnel = &tunnel;
                    let canceled = &canceled;
                    let progress = &progress;
                    scope.spawn(move || {
                        let site = probe_remote_browser_site(port, tunnel.port, canceled);
                        let _ = progress.unbounded_send(site.clone());
                        site
                    })
                })
                .collect();
            jobs.into_iter()
                .filter_map(|job| job.join().ok())
                .collect::<Vec<_>>()
        });
        sites.append(&mut results);
    }
    Ok(sites)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn discover_remote_browser_sites(
    _: &GpuiRemoteMachineConfig,
    _: &GpuiRemoteExecutionTarget,
    _: Arc<RemoteBrowserTunnel>,
    _: Arc<AtomicBool>,
    _: futures::channel::mpsc::UnboundedSender<RemoteBrowserSite>,
) -> Result<Vec<RemoteBrowserSite>, String> {
    Err("Remote port discovery is not available on this desktop platform yet.".into())
}

struct PageResponse {
    status: u16,
    content_type: String,
    body: Vec<u8>,
    location: Option<String>,
}

fn fetch_remote_page(url: &str, proxy_port: u16) -> Result<PageResponse, &'static str> {
    let mut child = Command::new("curl")
        .args([
            "--disable",
            "--silent",
            "--http1.1",
            "--include",
            "--max-time",
            "3",
            "--connect-timeout",
            "2",
            "--noproxy",
            "",
            "--socks5-hostname",
            &format!("127.0.0.1:{proxy_port}"),
            "--proto",
            "=http,https",
            "--url",
            url,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Could not run the web check")?;
    let mut bytes = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        let _ = stdout.take(512 * 1024).read_to_end(&mut bytes);
    }
    // Stop large/streaming bodies after enough HTML for the document head.
    if bytes.len() >= 512 * 1024 {
        let _ = child.kill();
    }
    let status = child.wait().ok().and_then(|status| status.code());
    let mut offset = 0;
    loop {
        let Some(end) = bytes[offset..]
            .windows(4)
            .position(|part| part == b"\r\n\r\n")
            .map(|end| end + offset)
        else {
            return Err(match status {
                Some(28) => "No response within 3s",
                Some(60) => "HTTPS certificate is not trusted",
                _ => "Web protocol unknown or unavailable",
            });
        };
        let headers = String::from_utf8_lossy(&bytes[offset..end]);
        let status = headers
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse::<u16>().ok())
            .filter(|code| (100..600).contains(code))
            .ok_or("Web protocol unknown")?;
        if status < 200 {
            offset = end + 4;
            continue;
        }
        let header = |name: &str| {
            headers
                .lines()
                .filter_map(|line| line.split_once(':'))
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.trim().to_string())
        };
        return Ok(PageResponse {
            status,
            content_type: header("content-type").unwrap_or_default(),
            location: header("location"),
            body: bytes[end + 4..].to_vec(),
        });
    }
}

fn probe_remote_browser_site(
    port: &GpuiRemoteListeningPort,
    proxy_port: u16,
    canceled: &AtomicBool,
) -> RemoteBrowserSite {
    let start = Instant::now();
    let mut site = RemoteBrowserSite {
        port: port.port,
        url: format!("http://localhost:{}/", port.port),
        title: None,
        process: port.process.clone(),
        favicon: None,
        status: None,
        can_open: false,
        detail: "Web protocol unknown".into(),
        checked_at: SystemTime::now(),
    };
    // Identify the protocol explicitly. Non-web TCP listeners remain visible as such.
    for scheme in ["http", "https"] {
        if canceled.load(Ordering::Relaxed) {
            break;
        }
        let url = format!("{scheme}://localhost:{}/", port.port);
        let mut response = match fetch_remote_page(&url, proxy_port) {
            Ok(response) => response,
            Err(detail) => {
                if scheme == "http" || detail.contains("certificate") {
                    site.detail = detail.into();
                }
                if scheme == "https" && detail.contains("certificate") {
                    site.url = url;
                    site.can_open = true;
                }
                continue;
            }
        };
        site.url = url.clone();
        site.status = Some(response.status);
        site.can_open = true;
        site.detail = format!(
            "HTTP {} · {} ms",
            response.status,
            start.elapsed().as_millis()
        );
        let mut document_url = Url::parse(&url).expect("constructed localhost URL");
        // Follow only same-port local redirects while identifying the page, without fetching an unrelated website's title.
        for _ in 0..3 {
            if !(300..400).contains(&response.status) {
                break;
            }
            let Some(next) = response
                .location
                .as_deref()
                .and_then(|location| document_url.join(location).ok())
                .filter(|next| {
                    remote_browser_loopback_url(next)
                        && next.port_or_known_default() == Some(port.port)
                })
            else {
                break;
            };
            let Ok(next_response) = fetch_remote_page(next.as_str(), proxy_port) else {
                break;
            };
            document_url = next;
            response = next_response;
        }
        if response.content_type.to_ascii_lowercase().contains("html") {
            let html = Html::parse_document(&String::from_utf8_lossy(&response.body));
            let title_selector = Selector::parse("title").expect("static selector");
            site.title = html.select(&title_selector).next().and_then(|title| {
                sanitize_browser_tab_cached_title(
                    &title
                        .text()
                        .collect::<String>()
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" "),
                )
            });
            let icon_selector = Selector::parse("link[rel][href]").expect("static selector");
            let icon_url = html
                .select(&icon_selector)
                .find(|link| {
                    link.value().attr("rel").is_some_and(|rel| {
                        rel.split_whitespace()
                            .any(|part| part.eq_ignore_ascii_case("icon"))
                    })
                })
                .and_then(|link| link.value().attr("href"))
                .and_then(|href| document_url.join(href).ok())
                .unwrap_or_else(|| document_url.join("/favicon.ico").expect("root URL"));
            if remote_browser_loopback_url(&icon_url)
                && icon_url.port_or_known_default() == Some(port.port)
            {
                if let Ok(icon) = fetch_remote_page(icon_url.as_str(), proxy_port) {
                    if (200..300).contains(&icon.status)
                        && icon.body.len() <= BROWSER_FAVICON_IMAGE_MAX_BYTES
                    {
                        if let Ok(format) =
                            browser_favicon_http_image_format(Some(&icon.content_type), &icon.body)
                        {
                            if browser_favicon_validate_encoded_dimensions(format, &icon.body)
                                .is_err()
                            {
                                break;
                            }
                            site.favicon = Some(BrowserFaviconImage {
                                image: Arc::new(Image::from_bytes(format, icon.body)),
                            });
                        }
                    }
                }
            }
        }
        break;
    }
    site.checked_at = SystemTime::now();
    site
}

fn remote_browser_loopback_url(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && url.username().is_empty()
        && url.password().is_none()
        && url
            .host_str()
            .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "[::1]"))
}

pub(crate) fn fetch_remote_browser_favicon(
    url: &str,
    proxy_port: u16,
) -> Option<BrowserFaviconImage> {
    let parsed = Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return None;
    }
    let response = fetch_remote_page(parsed.as_str(), proxy_port).ok()?;
    if !(200..300).contains(&response.status)
        || response.body.len() > BROWSER_FAVICON_IMAGE_MAX_BYTES
    {
        return None;
    }
    let format =
        browser_favicon_http_image_format(Some(&response.content_type), &response.body).ok()?;
    browser_favicon_validate_encoded_dimensions(format, &response.body).ok()?;
    Some(BrowserFaviconImage {
        image: Arc::new(Image::from_bytes(format, response.body)),
    })
}
