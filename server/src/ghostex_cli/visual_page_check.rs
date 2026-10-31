//! `ghostex visual check <page.html>`: opens a page the way `ghostex show` would publish it in a
//! headless Chrome at a phone's width and a computer's, saves a screenshot of each and says what
//! does not fit, so an agent sees its page on a phone before the user does. `ghostex show` runs the
//! phone half (no screenshot) after publishing and prints what it found.
//!
//! CDXC:SessionChat 2026-10-28 WHY:
//! The page kit (server/src/visual_pages/fit.rs) fits a too-wide page to the phone, but a page that needs fitting still reads worse than one written for a phone: a stacked comparison loses its side-by-side, a scrolled mockup hides its right half. The check reports what the kit had to fit (`window.__ghostexFit`) and anything still past the screen's edge, so the agent fixes its CSS before sending. Chrome is driven over the DevTools protocol because a headless Chrome window cannot be narrower than 500px, and the phone's width only comes from device emulation.
//! SEE-ALSO: server/src/visual_pages/fit.rs (`window.__ghostexFit`), skills/ghostex-visuals/SKILL.md (tells agents to run the check).

use std::{
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde_json::{json, Value};

use crate::ghostex_cli::rpc::{CliError, CliResult};
use crate::visual_pages::prepare_page;

/// The phone width pages are checked at: a common Android and iPhone width, inside the 360–430px
/// range phones have.
pub(crate) const PHONE_WIDTH: u32 = 390;
pub(crate) const DESKTOP_WIDTH: u32 = 1280;
/// Widths at or below this are emulated as a phone (meta viewport, touch-sized zoom).
const MOBILE_MAX_WIDTH: u32 = 600;
/// The tallest screenshot taken, in CSS pixels; a longer page is cut there.
const SCREENSHOT_MAX_HEIGHT: f64 = 8000.0;
const CHROME_START_TIMEOUT: Duration = Duration::from_secs(20);
const CDP_TIMEOUT: Duration = Duration::from_secs(20);

/// Whether `ghostex visual check` was given a page rather than a block's JSON.
pub(crate) fn is_page_file(file: &str) -> bool {
    let lower = file.to_ascii_lowercase();
    lower.ends_with(".html") || lower.ends_with(".htm")
}

pub(crate) struct PageCheck {
    pub(crate) width: u32,
    pub(crate) light: bool,
    pub(crate) screenshot: Option<PathBuf>,
}

/// What one width showed: the kit's fit report and the page's size.
pub(crate) struct PageCheckResult {
    pub(crate) width: u32,
    pub(crate) report: Value,
    pub(crate) screenshot: Option<PathBuf>,
}

/// `ghostex visual check <page.html> [--width N] [--light] [--out file.png]`.
pub(crate) fn check_page_command(
    file: &str,
    width: Option<u32>,
    light: bool,
    out: Option<PathBuf>,
) -> CliResult<()> {
    let path = absolute(file);
    let widths = width.map_or_else(|| vec![PHONE_WIDTH, DESKTOP_WIDTH], |width| vec![width]);
    let checks = widths
        .iter()
        .map(|&width| PageCheck {
            width,
            light,
            screenshot: Some(screenshot_path(
                &path,
                out.as_deref(),
                width,
                widths.len() > 1,
            )),
        })
        .collect::<Vec<_>>();
    let results = check_page(&path, &checks)?;
    for result in &results {
        let label = if result.width <= MOBILE_MAX_WIDTH {
            "Phone"
        } else {
            "Computer"
        };
        let shot = result
            .screenshot
            .as_ref()
            .map(|shot| format!(": {}", shot.display()))
            .unwrap_or_default();
        println!(
            "{label}, {}px wide ({} theme){shot}",
            result.width,
            if light { "light" } else { "dark" }
        );
        let findings = page_findings(result.width, &result.report);
        if findings.is_empty() {
            println!("  Fits: nothing reaches past the screen and the kit had nothing to fit.");
        }
        for finding in findings {
            println!("  - {finding}");
        }
    }
    // The Pick buttons do not depend on the width, so they are said once.
    if let Some(result) = results.first() {
        let picks = pick_findings(&result.report);
        if !picks.is_empty() {
            println!("Pick buttons:");
            for finding in picks {
                println!("  - {finding}");
            }
        }
    }
    println!("Look at each screenshot before sending the page.");
    Ok(())
}

/// What the check `ghostex show` runs after publishing found: the phone fit and the questions left
/// without Pick buttons.
#[derive(Default)]
pub(crate) struct ShowFindings {
    pub(crate) phone: Vec<String>,
    pub(crate) picks: Vec<String>,
}

/// The phone check `ghostex show` runs after publishing: what to fix, or nothing when the page fits
/// or no Chrome is installed to look with.
pub(crate) fn findings_for_show(path: &Path) -> ShowFindings {
    let check = PageCheck {
        width: PHONE_WIDTH,
        light: false,
        screenshot: None,
    };
    match check_page(path, &[check]) {
        Ok(results) => results
            .first()
            .map(|result| ShowFindings {
                phone: page_findings(result.width, &result.report),
                picks: pick_findings(&result.report),
            })
            .unwrap_or_default(),
        Err(_) => ShowFindings::default(),
    }
}

/// Prepares the page as `ghostex show` would and opens it once per width.
pub(crate) fn check_page(path: &Path, checks: &[PageCheck]) -> CliResult<Vec<PageCheckResult>> {
    let source = std::fs::read_to_string(path)
        .map_err(|error| CliError::Other(format!("Could not read {}: {error}", path.display())))?;
    let base_dir = path.parent().unwrap_or(Path::new("/"));
    let prepared = prepare_page(&source, base_dir).map_err(CliError::Other)?;
    let chrome = chrome_binary().ok_or_else(|| {
        CliError::Other(
            "Checking a page needs Google Chrome, Chromium, Edge or Brave; none was found. Set GHOSTEX_CHROME to the browser's program to use another one."
                .to_string(),
        )
    })?;
    let scratch = tempfile::Builder::new()
        .prefix("ghostex-page-check-")
        .tempdir()
        .map_err(|error| CliError::Other(format!("Could not make a scratch folder: {error}")))?;
    let page = scratch.path().join("page.html");
    std::fs::write(&page, prepared.html)
        .map_err(|error| CliError::Other(format!("Could not write the page to check: {error}")))?;
    let profile = scratch.path().join("profile");
    let browser = Browser::start(&chrome, &profile)?;
    let mut cdp = browser.connect()?;
    let mut results = Vec::new();
    for check in checks {
        results.push(check_width(&mut cdp, &page, check)?);
    }
    Ok(results)
}

fn check_width(cdp: &mut Cdp, page: &Path, check: &PageCheck) -> CliResult<PageCheckResult> {
    let mobile = check.width <= MOBILE_MAX_WIDTH;
    cdp.call(
        "Emulation.setDeviceMetricsOverride",
        json!({
            "width": check.width,
            "height": if mobile { 844 } else { 900 },
            "deviceScaleFactor": if mobile { 2 } else { 1 },
            "mobile": mobile,
        }),
    )?;
    let theme = if check.light { "light" } else { "dark" };
    let url = format!("{}#gx-theme={theme}", file_url(page));
    cdp.call("Page.navigate", json!({ "url": url }))?;
    let deadline = Instant::now() + CDP_TIMEOUT;
    let report = loop {
        let answer = cdp.call(
            "Runtime.evaluate",
            json!({ "expression": PROBE, "awaitPromise": true, "returnByValue": true }),
        )?;
        let value = answer
            .pointer("/result/value")
            .cloned()
            .unwrap_or(Value::Null);
        if value.get("page").and_then(Value::as_bool) == Some(true) {
            break value;
        }
        if Instant::now() >= deadline {
            return Err(CliError::Other(
                "The page did not finish loading in Chrome.".to_string(),
            ));
        }
        std::thread::sleep(Duration::from_millis(150));
    };
    let screenshot = match &check.screenshot {
        Some(out) => {
            let height = report
                .get("height")
                .and_then(Value::as_f64)
                .unwrap_or(900.0)
                .clamp(1.0, SCREENSHOT_MAX_HEIGHT);
            let shot = cdp.call(
                "Page.captureScreenshot",
                json!({
                    "format": "png",
                    "captureBeyondViewport": true,
                    "clip": { "x": 0, "y": 0, "width": check.width, "height": height, "scale": 1 },
                }),
            )?;
            let data = shot.get("data").and_then(Value::as_str).unwrap_or_default();
            let png = BASE64_STANDARD.decode(data).map_err(|error| {
                CliError::Other(format!("Chrome's screenshot could not be read: {error}"))
            })?;
            if let Some(parent) = out.parent().filter(|parent| !parent.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent).map_err(|error| {
                    CliError::Other(format!("Could not create {}: {error}", parent.display()))
                })?;
            }
            std::fs::write(out, png).map_err(|error| {
                CliError::Other(format!("Could not write {}: {error}", out.display()))
            })?;
            Some(out.clone())
        }
        None => None,
    };
    Ok(PageCheckResult {
        width: check.width,
        report,
        screenshot,
    })
}

/// Waits for the page to load and settle, then reads the kit's fit report and the page's size.
/// `page` is false while Chrome still shows the blank tab it started with.
const PROBE: &str = r#"(function(){
if(location.protocol!=="file:")return Promise.resolve({page:false});
return new Promise(function(done){
  function settle(){setTimeout(function(){requestAnimationFrame(function(){requestAnimationFrame(function(){
    var d=document.documentElement,fit=window.__ghostexFit||null,kit=window.__ghostexKit||null;
    done({page:true,fit:fit?JSON.parse(JSON.stringify(fit)):null,kit:kit?JSON.parse(JSON.stringify(kit)):null,docWidth:d.scrollWidth,viewport:d.clientWidth,height:Math.max(d.scrollHeight,document.body?document.body.scrollHeight:0)});
  });});},300);}
  if(document.readyState==="complete")settle();else addEventListener("load",settle,{once:true});
});
})()"#;

/// What to tell the agent about one width, from the probe's answer.
pub(crate) fn page_findings(width: u32, report: &Value) -> Vec<String> {
    let mut findings = Vec::new();
    let Some(fit) = report.get("fit").filter(|fit| !fit.is_null()) else {
        if report
            .get("docWidth")
            .and_then(Value::as_f64)
            .is_some_and(|wide| wide > f64::from(width) + 1.0)
        {
            findings.push(format!(
                "The page is wider than a {width}px screen. Phones zoom the whole page out to fit it, so everything turns small and narrow."
            ));
        }
        return findings;
    };
    let entries = |key: &str| -> Vec<String> {
        fit.get(key)
            .and_then(Value::as_array)
            .map(|items| items.iter().map(describe).collect())
            .unwrap_or_default()
    };
    let overflow = fit
        .get("overflow")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !overflow.is_empty() {
        let list = overflow
            .iter()
            .map(|item| {
                let right = item.get("right").and_then(Value::as_f64).unwrap_or(0.0);
                format!("{} (ends at {right:.0}px)", describe(item))
            })
            .collect::<Vec<_>>();
        findings.push(format!(
            "Reaches past the {width}px screen: {}. Make it fit the column: let it wrap or stack, give it max-width:100%, or put a fixed-width mockup in a .gx-frame (or <gx-wide>) so it scrolls sideways in its own box.",
            join(&list)
        ));
    }
    let stacked = entries("stacked");
    if !stacked.is_empty() {
        findings.push(format!(
            "The kit stacked side-by-side columns that did not fit: {}. Write a stacking rule yourself (.gx-grid, or @media (max-width:600px) {{ grid-template-columns: 1fr }}) so the page reads the same without the kit's help.",
            join(&stacked)
        ));
    }
    let nowrap = entries("nowrap");
    if !nowrap.is_empty() {
        findings.push(format!(
            "The kit kept pills from breaking mid-word: {}. Give pills, chips and badges white-space:nowrap and flex:none.",
            join(&nowrap)
        ));
    }
    let scroll = entries("scroll");
    if !scroll.is_empty() {
        findings.push(format!(
            "Scrolls sideways inside its own box (with a fade at the edge): {}. Fine for a fixed-width mockup of a real screen; anything else should wrap or stack to the column instead.",
            join(&scroll)
        ));
    }
    findings
}

/// The questions that sit under mockups although none of the mockups got a Pick button, from the
/// kit's report (`window.__ghostexKit.untied`, visual_pages/kit.rs `autoTie`): the author ties them
/// before sending, since the reader would have to scroll down to the question to pick.
///
/// CDXC:SessionChat 2026-10-30 DECISION:
/// User: "PLEASE for inline htmls we must show the pick button on the top right so i can pick one easily instead of having to pick below". The kit ties untied variants itself where it can; when it cannot, `ghostex show` and `ghostex visual check` say so plainly (a warning, never a refusal), naming the question.
pub(crate) fn pick_findings(report: &Value) -> Vec<String> {
    report
        .pointer("/kit/untied")
        .and_then(Value::as_array)
        .map(|questions| {
            questions
                .iter()
                .map(|question| {
                    let text = |key: &str| {
                        question
                            .get(key)
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|text| !text.is_empty())
                    };
                    let title = text("title");
                    let tag = text("tag");
                    let key = text("question").unwrap_or("a question");
                    let name = match (tag, title) {
                        (Some(tag), Some(title)) => format!("{tag} \"{title}\""),
                        (None, Some(title)) => format!("\"{title}\""),
                        (Some(tag), None) => tag.to_string(),
                        (None, None) => format!("\"{key}\""),
                    };
                    let asked = tag.unwrap_or(key);
                    format!(
                        "{name} sits under mockups but none of them got a Pick button, so the reader has to scroll down to the question to choose. Put data-question=\"{asked}\" and data-choice=\"<the choice's letter or label>\" on each variant's section (the element holding its heading and mockup), or start each variant's heading with its choice's letter (\"A. Tabs\") or exactly its label."
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn describe(item: &Value) -> String {
    let element = item
        .get("element")
        .and_then(Value::as_str)
        .unwrap_or("an element");
    match item
        .get("text")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
    {
        Some(text) => format!("{element} \"{text}\""),
        None => element.to_string(),
    }
}

/// At most six items, then how many more.
fn join(items: &[String]) -> String {
    const SHOWN: usize = 6;
    let mut text = items
        .iter()
        .take(SHOWN)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > SHOWN {
        text.push_str(&format!(" and {} more", items.len() - SHOWN));
    }
    text
}

/// Where a width's screenshot goes: `--out` as given for one width, with `-<width>` before the
/// extension for several, else beside the system's temporary files.
pub(crate) fn screenshot_path(
    page: &Path,
    out: Option<&Path>,
    width: u32,
    several: bool,
) -> PathBuf {
    match out {
        Some(out) if !several => out.to_path_buf(),
        Some(out) => {
            let stem = out
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| "page".to_string());
            out.with_file_name(format!("{stem}-{width}.png"))
        }
        None => {
            let stem = page
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| "page".to_string());
            std::env::temp_dir()
                .join("ghostex-visuals")
                .join(format!("{stem}-{width}.png"))
        }
    }
}

fn absolute(file: &str) -> PathBuf {
    let path = PathBuf::from(file);
    if path.is_absolute() {
        return path;
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(&path))
        .unwrap_or(path)
}

fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let encoded = text
        .split('/')
        .map(|part| {
            part.bytes()
                .map(|byte| match byte {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b':' => {
                        (byte as char).to_string()
                    }
                    _ => format!("%{byte:02X}"),
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("/");
    if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}

/// A Chromium browser to check with: `GHOSTEX_CHROME` when set, else the usual install places.
fn chrome_binary() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("GHOSTEX_CHROME")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
    {
        return Some(path);
    }
    chrome_candidates().into_iter().find(|path| path.is_file())
}

fn chrome_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if cfg!(target_os = "macos") {
        let apps = [
            "Google Chrome.app/Contents/MacOS/Google Chrome",
            "Chromium.app/Contents/MacOS/Chromium",
            "Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "Brave Browser.app/Contents/MacOS/Brave Browser",
        ];
        let mut roots = vec![PathBuf::from("/Applications")];
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(home).join("Applications"));
        }
        for root in &roots {
            for app in apps {
                candidates.push(root.join(app));
            }
        }
    } else if cfg!(windows) {
        let programs = [
            "Google/Chrome/Application/chrome.exe",
            "Microsoft/Edge/Application/msedge.exe",
            "BraveSoftware/Brave-Browser/Application/brave.exe",
            "Chromium/Application/chrome.exe",
        ];
        for root in ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"] {
            if let Some(root) = std::env::var_os(root) {
                for program in programs {
                    candidates.push(PathBuf::from(&root).join(program));
                }
            }
        }
    } else {
        let names = [
            "google-chrome",
            "google-chrome-stable",
            "chromium",
            "chromium-browser",
            "microsoft-edge",
            "brave-browser",
        ];
        if let Some(path) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&path) {
                for name in names {
                    candidates.push(dir.join(name));
                }
            }
        }
    }
    candidates
}

/// A headless Chrome with its own throwaway profile, stopped when dropped.
struct Browser {
    child: Child,
    port: u16,
}

impl Browser {
    fn start(chrome: &Path, profile: &Path) -> CliResult<Browser> {
        let child = Command::new(chrome)
            .args([
                "--headless=new",
                "--remote-debugging-port=0",
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-extensions",
                "--hide-scrollbars",
                "--mute-audio",
                "--allow-file-access-from-files",
            ])
            .arg(format!("--user-data-dir={}", profile.display()))
            .arg("about:blank")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                CliError::Other(format!("Could not start {}: {error}", chrome.display()))
            })?;
        let mut browser = Browser { child, port: 0 };
        let deadline = Instant::now() + CHROME_START_TIMEOUT;
        let active = profile.join("DevToolsActivePort");
        loop {
            if let Some(port) = std::fs::read_to_string(&active)
                .ok()
                .and_then(|text| text.lines().next()?.trim().parse::<u16>().ok())
            {
                browser.port = port;
                return Ok(browser);
            }
            if Instant::now() >= deadline || browser.child.try_wait().ok().flatten().is_some() {
                return Err(CliError::Other(format!(
                    "{} did not start a headless browser to check the page with.",
                    chrome.display()
                )));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// The DevTools socket of the browser's page tab (a new one when it shows none).
    fn connect(&self) -> CliResult<Cdp> {
        let deadline = Instant::now() + CHROME_START_TIMEOUT;
        loop {
            let pages = http(self.port, "GET", "/json/list").unwrap_or(Value::Null);
            let socket = pages.as_array().and_then(|pages| {
                pages
                    .iter()
                    .find(|page| page.get("type").and_then(Value::as_str) == Some("page"))
                    .and_then(|page| page.get("webSocketDebuggerUrl").and_then(Value::as_str))
                    .map(str::to_string)
            });
            let socket = socket.or_else(|| {
                http(self.port, "PUT", "/json/new?about:blank")
                    .ok()
                    .and_then(|page| {
                        page.get("webSocketDebuggerUrl")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
            });
            if let Some(socket) = socket {
                return Cdp::connect(self.port, &socket);
            }
            if Instant::now() >= deadline {
                return Err(CliError::Other(
                    "Chrome opened no page to check with.".to_string(),
                ));
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One small HTTP request to the browser's DevTools endpoint, answered with JSON (HTTP/1.1: Chrome
/// answers an HTTP/1.0 request with nothing).
fn http(port: u16, method: &str, path: &str) -> Result<Value, String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|error| error.to_string())?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 0\r\n\r\n"
    )
    .map_err(|error| error.to_string())?;
    // Chrome keeps the connection open after answering, so the body is read to its length.
    let mut answer = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    loop {
        let read = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        answer.extend_from_slice(&chunk[..read]);
        let text = String::from_utf8_lossy(&answer);
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let length = head.lines().find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.trim()
                    .eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())?
            });
            if length.is_some_and(|length| body.len() >= length) {
                break;
            }
        }
    }
    let text = String::from_utf8_lossy(&answer);
    let body = text.split_once("\r\n\r\n").map_or("", |(_, body)| body);
    serde_json::from_str(body).map_err(|error| error.to_string())
}

/// The page tab's DevTools socket: calls in order, events dropped.
struct Cdp {
    socket: tungstenite::WebSocket<TcpStream>,
    next_id: u64,
}

impl Cdp {
    fn connect(port: u16, url: &str) -> CliResult<Cdp> {
        let stream = TcpStream::connect(("127.0.0.1", port))
            .map_err(|error| CliError::Other(format!("Could not reach Chrome: {error}")))?;
        let (socket, _) = tungstenite::client::client(url, stream)
            .map_err(|error| CliError::Other(format!("Could not reach Chrome: {error}")))?;
        Ok(Cdp { socket, next_id: 1 })
    }

    fn call(&mut self, method: &str, params: Value) -> CliResult<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let payload = json!({ "id": id, "method": method, "params": params }).to_string();
        self.socket
            .send(tungstenite::Message::Text(payload.into()))
            .map_err(|error| CliError::Other(format!("Chrome stopped answering: {error}")))?;
        let deadline = Instant::now() + CDP_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(CliError::Other(format!(
                    "Chrome did not answer {method} in time."
                )));
            }
            let _ = self.socket.get_mut().set_read_timeout(Some(remaining));
            let message = self
                .socket
                .read()
                .map_err(|error| CliError::Other(format!("Chrome stopped answering: {error}")))?;
            let tungstenite::Message::Text(text) = message else {
                continue;
            };
            let Ok(parsed) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            if parsed.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = parsed.get("error") {
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error");
                return Err(CliError::Other(format!(
                    "Chrome refused {method}: {message}"
                )));
            }
            return Ok(parsed.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_files_are_pages() {
        assert!(is_page_file("mockup.html"));
        assert!(is_page_file("/a/B.HTM"));
        assert!(!is_page_file("chart.json"));
        assert!(!is_page_file("-"));
    }

    #[test]
    fn a_page_that_fits_has_no_findings() {
        let report = json!({"page":true,"docWidth":390,"fit":{"width":390,"stacked":[],"nowrap":[],"scroll":[],"overflow":[]}});
        assert!(page_findings(390, &report).is_empty());
    }

    #[test]
    fn what_the_kit_fitted_is_reported_with_how_to_fix_it() {
        let report = json!({"page":true,"fit":{
            "stacked":[{"element":"div.grid","text":"Today Proposed"}],
            "nowrap":[{"element":"span.pill","text":"Workers 2"}],
            "scroll":[{"element":"div.card","text":"Editing links.rs"}],
            "overflow":[]}});
        let findings = page_findings(390, &report);
        assert_eq!(findings.len(), 3);
        assert!(findings[0].contains("div.grid \"Today Proposed\""));
        assert!(findings[0].contains("stacking rule"));
        assert!(findings[1].contains("span.pill \"Workers 2\""));
        assert!(findings[1].contains("white-space:nowrap"));
        assert!(findings[2].contains("div.card"));
    }

    #[test]
    fn content_past_the_screen_comes_first_with_where_it_ends() {
        let report = json!({"page":true,"fit":{"stacked":[{"element":"div.grid"}],"nowrap":[],"scroll":[],
            "overflow":[{"element":"div.phone","text":"","right":612}]}});
        let findings = page_findings(390, &report);
        assert!(findings[0].starts_with("Reaches past the 390px screen: div.phone (ends at 612px)"));
        assert!(findings[1].contains("div.grid"));
    }

    #[test]
    fn a_page_without_the_kit_is_judged_by_its_width() {
        let wide = json!({"page":true,"fit":null,"docWidth":694});
        assert_eq!(page_findings(390, &wide).len(), 1);
        let fits = json!({"page":true,"fit":null,"docWidth":390});
        assert!(page_findings(390, &fits).is_empty());
    }

    #[test]
    fn long_lists_are_cut_short() {
        let items = (0..9).map(|n| format!("e{n}")).collect::<Vec<_>>();
        assert_eq!(join(&items), "e0, e1, e2, e3, e4, e5 and 3 more");
    }

    #[test]
    fn screenshots_get_the_width_when_several_are_taken() {
        let page = Path::new("/w/mock.html");
        assert_eq!(
            screenshot_path(page, Some(Path::new("/o/shot.png")), 390, false),
            PathBuf::from("/o/shot.png")
        );
        assert_eq!(
            screenshot_path(page, Some(Path::new("/o/shot.png")), 390, true),
            PathBuf::from("/o/shot-390.png")
        );
        assert!(screenshot_path(page, None, 1280, true).ends_with("ghostex-visuals/mock-1280.png"));
    }

    /// The kit's phone fit (visual_pages/fit.rs) in a real browser: a fixed two-column grid, pills
    /// that wrap and a fixed-width mockup. Runs only where a Chrome is installed.
    #[test]
    fn the_kit_fits_a_wide_page_to_a_phone_and_leaves_the_computer_alone() {
        if chrome_binary().is_none() {
            eprintln!("skipped: no Chrome to check pages with");
            return;
        }
        let dir = tempfile::tempdir().expect("a scratch folder");
        let page = dir.path().join("wide.html");
        std::fs::write(
            &page,
            r#"<!doctype html><html><head><meta name="ghostex-page" content="v2"><title>Wide</title>
<style>body{padding:20px}.two{display:grid;grid-template-columns:1fr 1fr;gap:14px}
.row{display:flex;gap:8px;white-space:nowrap}.pill{display:inline-flex;padding:0 10px;border:1px solid;border-radius:13px}
.phone{width:600px;height:40px}</style></head><body><gx-page>
<header><h1>Wide</h1></header>
<div class="two"><div class="row">A row that is long enough to overflow a phone column</div><div class="row">Another long row of words here</div></div>
<div class="bar" style="display:flex;gap:6px"><span class="pill">Workers 2</span><span class="pill">SPX-123 +2</span><span class="pill">Resources 3</span><span class="pill">Tasks 6/11</span><span class="pill">Workers 1</span><span class="pill">3 suggested</span></div>
<div class="phone">fixed-width mockup</div>
</gx-page></body></html>"#,
        )
        .expect("the page");
        let checks = [PHONE_WIDTH, DESKTOP_WIDTH].map(|width| PageCheck {
            width,
            light: false,
            screenshot: None,
        });
        let results = check_page(&page, &checks).expect("the check");
        let fit = |index: usize, key: &str| {
            results[index].report["fit"][key]
                .as_array()
                .cloned()
                .unwrap_or_default()
        };
        let phone = &results[0].report;
        assert!(phone["docWidth"].as_f64().unwrap() <= f64::from(PHONE_WIDTH) + 1.0);
        assert!(fit(0, "overflow").is_empty(), "{phone}");
        assert_eq!(fit(0, "stacked").len(), 1, "{phone}");
        assert!(fit(0, "nowrap").len() >= 2, "{phone}");
        assert!(
            fit(0, "scroll")
                .iter()
                .any(|item| item["element"] == "div.phone"),
            "{phone}"
        );
        let computer = &results[1].report;
        assert!(
            fit(1, "stacked").is_empty() && fit(1, "nowrap").is_empty(),
            "{computer}"
        );
        assert!(fit(1, "overflow").is_empty(), "{computer}");
    }

    /// Opens a page the way `ghostex show` would publish it in a headless Chrome at the computer's
    /// width, then evaluates the expression there. `None` where no Chrome is installed.
    fn eval_in_page(body: &str, expression: &str) -> Option<Value> {
        let chrome = chrome_binary().or_else(|| {
            eprintln!("skipped: no Chrome to check pages with");
            None
        })?;
        let dir = tempfile::tempdir().expect("a scratch folder");
        let html = format!(
            "<!doctype html><html><head><meta name=\"ghostex-page\" content=\"v2\"><title>Pick</title></head><body>{body}</body></html>"
        );
        let prepared = prepare_page(&html, dir.path()).expect("a prepared page");
        let page = dir.path().join("page.html");
        std::fs::write(&page, prepared.html).expect("the page");
        let browser = Browser::start(&chrome, &dir.path().join("profile")).expect("a browser");
        let mut cdp = browser.connect().expect("a connection");
        cdp.call(
            "Emulation.setDeviceMetricsOverride",
            json!({ "width": DESKTOP_WIDTH, "height": 900, "deviceScaleFactor": 1, "mobile": false }),
        )
        .expect("a window size");
        cdp.call("Page.navigate", json!({ "url": file_url(&page) }))
            .expect("the page opens");
        let deadline = Instant::now() + CDP_TIMEOUT;
        loop {
            let answer = cdp
                .call(
                    "Runtime.evaluate",
                    json!({ "expression": PROBE, "awaitPromise": true, "returnByValue": true }),
                )
                .expect("the probe");
            if answer
                .pointer("/result/value/page")
                .and_then(Value::as_bool)
                == Some(true)
            {
                break;
            }
            assert!(Instant::now() < deadline, "the page did not load");
            std::thread::sleep(Duration::from_millis(150));
        }
        let answer = cdp
            .call(
                "Runtime.evaluate",
                json!({ "expression": expression, "returnByValue": true }),
            )
            .expect("the expression");
        let text = answer
            .pointer("/result/value")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("no answer: {answer}"));
        Some(serde_json::from_str(text).expect("JSON"))
    }

    /// What the kit tied and what is on the page, as JSON text.
    const PICKS: &str = r#"JSON.stringify({kit:window.__ghostexKit,picks:Array.prototype.map.call(document.querySelectorAll(".gx-pick"),function(p){var h=p.parentElement;return{text:h.textContent.replace(/Pick$/,"").trim(),tag:h.tagName,last:h.lastElementChild===p};})})"#;

    fn question(tag: &str, title: &str, choices: &[&str]) -> String {
        let choices = choices
            .iter()
            .map(|choice| format!("<gx-choice>{choice}</gx-choice>"))
            .collect::<String>();
        format!("<gx-question tag=\"{tag}\" title=\"{title}\">{choices}</gx-question>")
    }

    fn variants(headings: &[&str]) -> String {
        let sections = headings
            .iter()
            .map(|heading| {
                format!("<section><h3>{heading}</h3><div class=\"gx-frame\"><div style=\"width:300px;height:80px\">mockup</div></div></section>")
            })
            .collect::<String>();
        format!("<gx-wide>{sections}</gx-wide>")
    }

    fn pairs(list: &Value) -> Vec<(String, String)> {
        list.as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|item| {
                (
                    item["question"].as_str().unwrap_or_default().to_string(),
                    format!(
                        "{}={}",
                        item["heading"].as_str().unwrap_or_default(),
                        item["choice"].as_str().unwrap_or_default()
                    ),
                )
            })
            .collect()
    }

    /// The matching of untied variants to choices (visual_pages/kit.rs `autoTie`): by letter, by
    /// label, never for an ambiguous or unmatched heading, never for a heading of an earlier
    /// question, and not at all for a question that has tied variants.
    #[test]
    fn untied_variants_get_pick_buttons_only_where_the_match_is_certain() {
        let body = format!(
            "<gx-questions><header><h2>Header heading</h2></header>{q1v}{q1}{q2v}{q2}{q3v}{q3}{q4v}{q4}{q5v}{q5}{q6v}{q6}<h3>C. Stray after the last question</h3>{q7}</gx-questions>",
            q1v = variants(&["A. Whatever the first looks like", "B. Something else again"]),
            q1 = question("Q1", "Which layout?", &["Tabs", "One long page"]),
            q2v = variants(&["Compact", "Roomy"]),
            q2 = question("Q2", "How dense?", &["Compact", "Roomy"]),
            q3v = variants(&["A. Red", "A. Green"]),
            q3 = question("Q3", "Which colour?", &["Red", "Green"]),
            q4v = variants(&["Alpha", "Alpha"]),
            q4 = question("Q4", "Which name?", &["Alpha", "Beta"]),
            q5v = variants(&["Unrelated heading"]),
            q5 = question("Q5", "Which other?", &["One", "Two"]),
            q6v = "<section data-question=\"Q6\" data-choice=\"x\"><h3>A. Already tied</h3><div class=\"gx-frame\">m</div></section><section><h3>B. Not tied</h3><div class=\"gx-frame\">m</div></section>",
            q6 = question("Q6", "Which tied?", &["x", "y"]),
            q7 = question("Q7", "Anything with no mockups?", &["Yes", "No"]),
        );
        let Some(page) = eval_in_page(&body, PICKS) else {
            return;
        };
        let kit = &page["kit"];
        assert_eq!(
            pairs(&kit["auto"]),
            vec![
                (
                    "Which layout?".into(),
                    "A. Whatever the first looks like=a".into()
                ),
                ("Which layout?".into(), "B. Something else again=b".into()),
                ("How dense?".into(), "Compact=a".into()),
                ("How dense?".into(), "Roomy=b".into()),
                ("Which colour?".into(), "A. Red=a".into()),
            ],
            "{page}"
        );
        let untied = kit["untied"]
            .as_array()
            .expect("untied")
            .iter()
            .map(|item| item["tag"].as_str().unwrap_or_default().to_string())
            .collect::<Vec<_>>();
        assert_eq!(untied, vec!["Q4", "Q5"], "{page}");
        // Q6 is tied by its author: the kit leaves its other heading alone.
        assert_eq!(kit["tied"], 1, "{page}");
        let picks = page["picks"].as_array().expect("picks");
        let texts = picks
            .iter()
            .map(|pick| pick["text"].as_str().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(
            texts,
            vec![
                "A. Whatever the first looks like",
                "B. Something else again",
                "Compact",
                "Roomy",
                "A. Red",
                "A. Already tied",
            ],
            "{page}"
        );
        // Each button is the last thing in its heading's row.
        assert!(picks
            .iter()
            .all(|pick| pick["last"] == true && pick["tag"] == "H3"));
        // The messages name the questions.
        let report = json!({ "kit": kit });
        let findings = pick_findings(&report);
        assert_eq!(findings.len(), 2);
        assert!(findings[0].contains("Q4 \"Which name?\""), "{findings:?}");
        assert!(findings[1].contains("data-question=\"Q5\""), "{findings:?}");
    }

    /// A tied heading's Pick button picks its choice and shows it picked in both places; clicking
    /// either one clears both.
    #[test]
    fn an_auto_tied_pick_and_its_choice_pick_and_clear_together() {
        let body = format!(
            "<gx-questions>{}{}</gx-questions>",
            variants(&["A. Tabs", "B. One long page"]),
            question("Q1", "Which layout?", &["Tabs", "One long page"])
        );
        let expression = r#"(function(){
          var picks=document.querySelectorAll(".gx-pick"),radios=document.querySelectorAll(".gx-choice"),log=[];
          function state(){return Array.prototype.map.call(picks,function(p){return p.getAttribute("aria-pressed")+":"+p.textContent;}).concat(Array.prototype.map.call(radios,function(r){return r.getAttribute("aria-checked");}));}
          log.push(state());picks[1].click();log.push(state());picks[1].click();log.push(state());
          radios[0].click();log.push(state());picks[1].click();log.push(state());radios[1].click();log.push(state());
          return JSON.stringify(log);})()"#;
        let Some(log) = eval_in_page(&body, expression) else {
            return;
        };
        let log: Vec<Vec<String>> = serde_json::from_value(log).expect("states");
        // Order: pick A, pick B, choice a, choice b, Other.
        assert_eq!(
            log[0],
            ["false:Pick", "false:Pick", "false", "false", "false"]
        );
        assert_eq!(
            log[1],
            [
                "false:Pick",
                "true:\u{2713} Picked",
                "false",
                "true",
                "false"
            ]
        );
        assert_eq!(
            log[2],
            ["false:Pick", "false:Pick", "false", "false", "false"]
        );
        assert_eq!(
            log[3],
            [
                "true:\u{2713} Picked",
                "false:Pick",
                "true",
                "false",
                "false"
            ]
        );
        assert_eq!(
            log[4],
            [
                "false:Pick",
                "true:\u{2713} Picked",
                "false",
                "true",
                "false"
            ]
        );
        assert_eq!(
            log[5],
            ["false:Pick", "false:Pick", "false", "false", "false"]
        );
    }

    /// Pages whose variants are tied by their author behave as before: the kit ties nothing more
    /// and says nothing.
    #[test]
    fn variants_tied_by_the_author_are_left_exactly_as_they_were() {
        let body = "<gx-questions><gx-wide><section data-question=\"Q1\" data-choice=\"a\"><h3>A. Tabs</h3><div class=\"gx-frame\">m</div></section><section data-question=\"Q1\" data-choice=\"One long page\"><h3>Second</h3><div class=\"gx-frame\">m</div></section></gx-wide>".to_string()
            + &question("Q1", "Which layout?", &["Tabs", "One long page"])
            + "</gx-questions>";
        let Some(page) = eval_in_page(&body, PICKS) else {
            return;
        };
        assert_eq!(page["kit"]["tied"], 2, "{page}");
        assert!(page["kit"]["auto"].as_array().unwrap().is_empty(), "{page}");
        assert!(
            page["kit"]["untied"].as_array().unwrap().is_empty(),
            "{page}"
        );
        assert_eq!(page["picks"].as_array().unwrap().len(), 2, "{page}");
    }

    #[test]
    fn a_question_without_a_pick_is_named_and_a_tied_page_is_quiet() {
        let report = json!({"page":true,"kit":{"tied":0,"auto":[],"untied":[
            {"question":"Which layout?","tag":"Q1","title":"Which layout?"},
            {"question":"Plain","tag":"","title":""}]}});
        let findings = pick_findings(&report);
        assert_eq!(findings.len(), 2);
        assert!(findings[0].starts_with("Q1 \"Which layout?\" sits under mockups"));
        assert!(findings[0].contains("data-question=\"Q1\""));
        assert!(findings[1].starts_with("\"Plain\" sits under mockups"));
        assert!(
            pick_findings(&json!({"page":true,"kit":{"tied":1,"auto":[],"untied":[]}})).is_empty()
        );
        assert!(pick_findings(&json!({"page":true,"kit":null})).is_empty());
    }

    #[test]
    fn file_urls_escape_spaces() {
        assert_eq!(
            file_url(Path::new("/tmp/a b/page.html")),
            "file:///tmp/a%20b/page.html"
        );
    }
}
