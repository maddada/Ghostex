//! Answers what a web page asks of the computer (`cef/shell/site_requests.rs`): opening another app
//! through its link, and connecting to apps on this computer or devices on the local network. Each
//! is a question at the top of the page that asked (`element/cef_surface_site_prompt.rs`), the way
//! Chrome asks, and never blocks the window.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;
use std::time::{Duration, Instant};

use futures::future::{Either, select};
use gpui::{App, AsyncApp, WeakEntity};

use crate::BrowserProfileId;
use crate::app::element::{CefSurface, SitePromptContent, SitePromptIcon, cef_surface_for_browser};

use crate::cef::{BrowserExternalAppRequest, BrowserLocalNetworkAccessRequest, BrowserSiteRequest};

/// A page that asks again this soon after the user said Don't Open or not now is not asked, so a
/// page retrying in a loop cannot keep putting the question back; a deliberate second click still
/// asks.
const DECLINED_EXTERNAL_APP_QUIET_PERIOD: Duration = Duration::from_millis(1500);

thread_local! {
    static DECLINED_EXTERNAL_APPS: RefCell<HashMap<String, Instant>> = RefCell::new(HashMap::new());
}

pub(crate) fn register_browser_site_request_handler(cx: &App) {
    let async_cx = cx.to_async();
    crate::cef::set_browser_site_request_handler(Rc::new(move |request| {
        // CEF calls this from inside its message pump; the question opens on the app's next turn.
        async_cx
            .spawn(async move |cx| answer_browser_site_request(request, cx).await)
            .detach();
    }));
}

async fn answer_browser_site_request(request: BrowserSiteRequest, cx: &mut AsyncApp) {
    match request {
        BrowserSiteRequest::OpenExternalApp(request) => {
            answer_external_app_request(request, cx).await
        }
        BrowserSiteRequest::LocalNetworkAccess(request) => {
            answer_local_network_access_request(request, cx).await
        }
    }
}

/// CDXC:Browser 2026-10-03 WHY:
/// A link only another app opens (Okta Verify's `com-okta-authenticator:`, Zoom, Teams, `mailto:`)
/// asks first, naming the app, as Chrome does; a link no installed app handles is dropped, since
/// the OS would only show an error. Chrome's "always allow" checkbox is left out: Ghostex has no
/// site-settings page to take it back.
async fn answer_external_app_request(mut request: BrowserExternalAppRequest, cx: &mut AsyncApp) {
    let declined_key = format!("{}|{}", request.origin, request.scheme);
    let recently_declined = DECLINED_EXTERNAL_APPS.with(|declined| {
        declined
            .borrow()
            .get(&declined_key)
            .is_some_and(|at| at.elapsed() < DECLINED_EXTERNAL_APP_QUIET_PERIOD)
    });
    if recently_declined {
        return;
    }
    let Some(surface) = cef_surface_for_browser(request.browser_id()) else {
        return;
    };
    let (url, scheme) = (request.url.clone(), request.scheme.clone());
    let Some(app_name) = cx
        .background_executor()
        .spawn(async move { registered_app_for_link(&url, &scheme) })
        .await
    else {
        return;
    };
    let site = site_label(&request.origin);
    let (message, open_label) = match app_name {
        Some(app) => (
            format!("{site} wants to open {app}."),
            format!("Open {app}"),
        ),
        None => (
            format!(
                "{site} wants to open a {}: link in another app.",
                request.scheme
            ),
            "Open".to_string(),
        ),
    };
    let page_gone = request.page_gone();
    let answer = ask_on_page(
        &surface,
        SitePromptContent {
            key: format!("app|{declined_key}"),
            icon: SitePromptIcon::ExternalApp,
            message,
            detail: None,
            allow_label: open_label,
            deny_label: "Don't Open".to_string(),
        },
        page_gone,
        cx,
    )
    .await;
    match answer {
        Some(Some(true)) => cx.update(|cx| cx.open_url(&request.url)),
        Some(_) => DECLINED_EXTERNAL_APPS.with(|declined| {
            let mut declined = declined.borrow_mut();
            declined.retain(|_, at| at.elapsed() < DECLINED_EXTERNAL_APP_QUIET_PERIOD);
            declined.insert(declined_key, Instant::now());
        }),
        None => {}
    }
}

/// CDXC:Browser 2026-10-10 WHY:
/// Chromium's Local Network Access asks before a public page reaches 127.0.0.1 or the local
/// network; sign-in pages use it to reach a desktop authenticator (Okta FastPass probes Okta
/// Verify's server on 127.0.0.1), and linear.app uses it to look for the Linear desktop app before
/// sending its page there. Allow and Don't Allow are both kept for the site in that browser
/// profile, as Chrome does, so Don't Allow keeps Linear on its web page; Settings > Workspaces
/// forgets them (`forget_browser_site_answers`). Supersedes 2026-10-03, when Don't Allow only
/// dismissed because there was no way to undo a block. The × is "not now" and keeps nothing, and
/// the question is taken down unanswered when its page goes away, so it never outlives its tab.
async fn answer_local_network_access_request(
    mut request: BrowserLocalNetworkAccessRequest,
    cx: &mut AsyncApp,
) {
    let Some(surface) = cef_surface_for_browser(request.browser_id()) else {
        return;
    };
    let site = site_label(request.origin());
    let (message, detail) = if request.includes_local_network() {
        (
            format!("Allow {site} to connect to devices on your local network?"),
            "Only allow sites you trust.",
        )
    } else {
        (
            format!("Allow {site} to connect to apps on this computer?"),
            "Sign-in pages use this to reach an authenticator app such as Okta Verify. Only allow sites you trust.",
        )
    };
    let page_gone = request.page_gone();
    let answer = ask_on_page(
        &surface,
        SitePromptContent {
            key: format!("network|{}", request.origin()),
            icon: SitePromptIcon::LocalNetwork,
            message,
            detail: Some(detail.to_string()),
            allow_label: "Allow".to_string(),
            deny_label: "Don't Allow".to_string(),
        },
        page_gone,
        cx,
    )
    .await;
    // Dropping the request unanswered answers the page "not now". Chromium still counts it toward
    // blocking the site by itself, so the site is listed for Forget all answers either way.
    match answer {
        Some(Some(allow)) => {
            remember_browser_site_answer(request.profile(), request.origin());
            request.answer(allow);
        }
        Some(None) => remember_browser_site_answer(request.profile(), request.origin()),
        None => {}
    }
}

/// Shows a question on the page that asked and waits for it. None when it was not shown (the page
/// has the same question up already, or is gone) or its page went away; `Some(None)` for "not
/// now" (the ×, Escape, or the page's surface dropped with its tab or window); `Some(Some(_))` for
/// Allow or Don't Allow.
async fn ask_on_page(
    surface: &WeakEntity<CefSurface>,
    content: SitePromptContent,
    page_gone: impl std::future::Future<Output = ()>,
    cx: &mut AsyncApp,
) -> Option<Option<bool>> {
    let (prompt_id, answer) = surface
        .update(cx, |surface, cx| surface.push_site_prompt(content, cx))
        .ok()
        .flatten()?;
    match select(answer, std::pin::pin!(page_gone)).await {
        Either::Left((answer, _)) => Some(answer.ok()),
        Either::Right(_) => {
            let _ = surface.update(cx, |surface, cx| surface.remove_site_prompt(prompt_id, cx));
            None
        }
    }
}

/// Sites that have an Allow or Don't Allow kept in a disk-backed browser profile, by CEF profile,
/// so Settings can forget them: Chromium has no way to list them.
fn browser_site_answers_path() -> std::path::PathBuf {
    crate::ghostex_state_root().join("gpui-browser-site-answers.json")
}

fn read_browser_site_answers() -> BTreeMap<String, BTreeSet<String>> {
    std::fs::read_to_string(browser_site_answers_path())
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| value.get("localNetwork").cloned())
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

fn write_browser_site_answers(answers: &BTreeMap<String, BTreeSet<String>>) {
    let path = browser_site_answers_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let text = serde_json::json!({ "localNetwork": answers }).to_string();
    let _ = std::fs::write(path, text);
}

/// The key a browser profile's answers are kept under: app pages (extension views, modals) share
/// the Default profile's browser context. None for memory-backed profiles, whose answers end with
/// the app.
fn browser_site_answers_profile(profile: &str) -> Option<String> {
    let segment = crate::cef::cef_profile_cache_segment(profile)?;
    if crate::cef::cef_profile_is_workspace(segment) {
        return Some(segment.to_string());
    }
    let default_profile = BrowserProfileId::default_profile().cef_profile_string();
    (segment == default_profile || crate::cef::cef_profile_is_app_ui(segment))
        .then_some(default_profile)
}

fn remember_browser_site_answer(profile: &str, origin: &str) {
    let Some(profile) = browser_site_answers_profile(profile) else {
        return;
    };
    let mut answers = read_browser_site_answers();
    if answers
        .entry(profile)
        .or_default()
        .insert(origin.to_string())
    {
        write_browser_site_answers(&answers);
    }
}

/// Forgets every site's Allow or Don't Allow in a browser profile, so each site asks again.
/// Returns how many sites were forgotten.
pub(crate) fn forget_browser_site_answers(profile: &str) -> Result<usize, String> {
    let Some(profile) = browser_site_answers_profile(profile) else {
        return Ok(0);
    };
    let mut answers = read_browser_site_answers();
    let Some(origins) = answers.remove(&profile) else {
        return Ok(0);
    };
    let mut kept = BTreeSet::new();
    for origin in &origins {
        if !crate::cef::forget_local_network_access_answer(&profile, origin) {
            kept.insert(origin.clone());
        }
    }
    let forgotten = origins.len() - kept.len();
    if !kept.is_empty() {
        answers.insert(profile, kept);
    }
    write_browser_site_answers(&answers);
    if forgotten == 0 && !origins.is_empty() {
        return Err("This workspace's Browser isn't ready yet. Try again in a moment.".into());
    }
    Ok(forgotten)
}

/// `acme.okta.com` from `https://acme.okta.com` (Local Network Access hands over `https://linear.app/`).
fn site_label(origin: &str) -> String {
    match origin
        .split_once("://")
        .map(|(_, rest)| rest.trim_end_matches('/'))
    {
        Some(authority) if !authority.is_empty() => authority.to_string(),
        _ => "This page".to_string(),
    }
}

/// The app the OS opens `url` with: None when no app is registered for it, `Some(None)` when one is
/// but the OS does not name it.
#[cfg(target_os = "macos")]
fn registered_app_for_link(url: &str, _scheme: &str) -> Option<Option<String>> {
    crate::app::native_docs::open_externally::open_with_applications(url, true)
        .into_iter()
        .next()
        .map(|app| Some(app.name))
}

#[cfg(target_os = "windows")]
fn registered_app_for_link(_url: &str, scheme: &str) -> Option<Option<String>> {
    use windows_sys::Win32::System::Registry::{HKEY_CLASSES_ROOT, RRF_RT_ANY, RegGetValueW};
    use windows_sys::Win32::UI::Shell::{
        ASSOCF_INIT_IGNOREUNKNOWN, ASSOCF_IS_PROTOCOL, ASSOCSTR_FRIENDLYAPPNAME, AssocQueryStringW,
    };
    let wide = |text: &str| text.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let scheme = wide(scheme);
    let mut name = [0u16; 260];
    let mut len = name.len() as u32;
    // SAFETY: both strings are NUL-terminated and `len` is the buffer's length in characters.
    let named = unsafe {
        AssocQueryStringW(
            ASSOCF_IS_PROTOCOL | ASSOCF_INIT_IGNOREUNKNOWN,
            ASSOCSTR_FRIENDLYAPPNAME,
            scheme.as_ptr(),
            std::ptr::null(),
            name.as_mut_ptr(),
            &mut len,
        )
    } >= 0;
    if named {
        let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
        let name = String::from_utf16_lossy(&name[..end]).trim().to_string();
        return Some((!name.is_empty()).then_some(name));
    }
    // A protocol registered under HKCR whose app Windows cannot name.
    let value = wide("URL Protocol");
    // SAFETY: NUL-terminated key and value names; no data is read back.
    let registered = unsafe {
        RegGetValueW(
            HKEY_CLASSES_ROOT,
            scheme.as_ptr(),
            value.as_ptr(),
            RRF_RT_ANY,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0;
    registered.then_some(None)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn registered_app_for_link(_url: &str, scheme: &str) -> Option<Option<String>> {
    let output = std::process::Command::new("xdg-mime")
        .args(["query", "default", &format!("x-scheme-handler/{scheme}")])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            (!String::from_utf8_lossy(&output.stdout).trim().is_empty()).then_some(None)
        }
        // Without xdg-mime there is no way to tell; xdg-open decides.
        _ => Some(None),
    }
}
