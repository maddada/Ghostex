//! A web page asking for something outside the page: opening another app through its link scheme
//! (`com-okta-authenticator:`, `zoommtg:`, `mailto:`), or connecting to apps on this computer and
//! devices on the local network. Alloy-style CEF refuses both without a word, so every CEF page
//! hands them to the app, which asks the user (`app/browser_site_requests.rs`).
use super::*;

/// CDXC:Browser 2026-10-03 WHY:
/// Every Ghostex CEF browser is Alloy style (a native child view always is), and Alloy drops an
/// unknown-scheme navigation unless `OnProtocolExecution` allows it and answers a permission prompt
/// it has no handler for with IGNORE. Okta FastPass (Okta Verify on the desktop) needs both: it
/// probes Okta Verify's server on 127.0.0.1, which Chromium's Local Network Access gates behind a
/// `loopback-network` prompt, and falls back to loading `com-okta-authenticator:/deviceChallenge…`
/// in a hidden iframe, which is also what its "Open Okta Verify" button does. So the Linear
/// extension's Okta SSO never reached Okta Verify (Discord report, 10.8.1). Chrome asks before
/// either; so does Ghostex. CEF never launches the app itself (`allow_os_execution` stays 0): the
/// app checks that the OS has an app for the link, asks, and opens it, the same way for Browser
/// panes, website and custom views, extension views, modals and panels.
/// SEE-ALSO: app/browser_site_requests.rs (the prompts), GhostexGpuiPermissionHandler (the
/// Local Network Access prompt), request_handling.rs and browser_handlers.rs (the popup paths).
pub enum BrowserSiteRequest {
    OpenExternalApp(BrowserExternalAppRequest),
    LocalNetworkAccess(BrowserLocalNetworkAccessRequest),
}

pub struct BrowserExternalAppRequest {
    pub url: String,
    /// Lowercase, without the colon.
    pub scheme: String,
    /// The asking page's `scheme://host[:port]`, empty when it is not an http(s) page.
    pub origin: String,
    /// The asking browser (`CefSurface::browser_identifier`), whose page shows the question.
    browser_id: i32,
    /// Closes the Browser tab that was opened only for this link; runs once the request is done
    /// with (answered, skipped or not shown), so it never outlives the prompt.
    close_link_only_tab: Option<BrowserPageMetadataHandler>,
    page_watch: SitePromptPageWatch,
}

impl BrowserExternalAppRequest {
    fn new(
        url: String,
        scheme: String,
        origin: String,
        browser_id: i32,
        close_link_only_tab: Option<BrowserPageMetadataHandler>,
    ) -> Self {
        let page_watch = SitePromptPageWatch::register(browser_id, None, Some(origin.clone()));
        Self {
            url,
            scheme,
            origin,
            browser_id,
            close_link_only_tab,
            page_watch,
        }
    }

    pub fn browser_id(&self) -> i32 {
        self.browser_id
    }

    /// Resolves when the asking page is gone (its browser closed, or its main frame left the
    /// asking site), so the app can take its question down; never resolves otherwise.
    pub fn page_gone(&mut self) -> impl std::future::Future<Output = ()> + 'static {
        self.page_watch.page_gone()
    }
}

impl Drop for BrowserExternalAppRequest {
    fn drop(&mut self) {
        if let Some(handler) = self.close_link_only_tab.take() {
            handler(BrowserPageMetadataEvent::CloseRequested);
        }
    }
}

/// A pending Local Network Access prompt. It stays open until it is answered or dropped, and
/// dropping it unanswered answers "not now" (DISMISS) so the page's request never hangs.
pub struct BrowserLocalNetworkAccessRequest {
    pub(crate) origin: String,
    pub(crate) local_network: bool,
    /// The CEF profile of the asking browser, whose request context keeps the answer.
    pub(crate) profile: String,
    /// The Chromium content settings this prompt is for (loopback and/or local network).
    pub(crate) content_types: Vec<ContentSettingTypes>,
    pub(crate) browser_id: i32,
    pub(crate) callback: Option<PermissionPromptCallback>,
    pub(crate) page_watch: SitePromptPageWatch,
}

impl BrowserLocalNetworkAccessRequest {
    pub fn origin(&self) -> &str {
        &self.origin
    }

    pub fn profile(&self) -> &str {
        &self.profile
    }

    /// True when the page asked for devices on the local network, not only apps on this computer.
    pub fn includes_local_network(&self) -> bool {
        self.local_network
    }

    pub fn browser_id(&self) -> i32 {
        self.browser_id
    }

    /// Resolves when the asking page is gone (its tab or browser closed, or it navigated away),
    /// so the app can take its question down; never resolves otherwise.
    pub fn page_gone(&mut self) -> impl std::future::Future<Output = ()> + 'static {
        self.page_watch.page_gone()
    }

    /// Allow or Don't Allow, kept for the site in this browser profile as Chrome does, so the
    /// site is not asked again until the answer is forgotten
    /// (`forget_local_network_access_answer`).
    pub fn answer(mut self, allow: bool) {
        set_local_network_access_setting(
            &self.profile,
            &self.origin,
            &self.content_types,
            if allow {
                ContentSettingValues::ALLOW
            } else {
                ContentSettingValues::BLOCK
            },
        );
        if let Some(callback) = self.callback.take() {
            callback.cont(if allow {
                PermissionRequestResult::ACCEPT
            } else {
                PermissionRequestResult::DENY
            });
        }
    }
}

impl Drop for BrowserLocalNetworkAccessRequest {
    fn drop(&mut self) {
        if let Some(callback) = self.callback.take() {
            callback.cont(PermissionRequestResult::DISMISS);
        }
    }
}

const LOCAL_NETWORK_ACCESS_CONTENT_TYPES: [ContentSettingTypes; 3] = [
    ContentSettingTypes::LOOPBACK_NETWORK,
    ContentSettingTypes::LOCAL_NETWORK,
    ContentSettingTypes::LOCAL_NETWORK_ACCESS,
];

fn set_local_network_access_setting(
    profile: &str,
    origin: &str,
    content_types: &[ContentSettingTypes],
    value: ContentSettingValues,
) -> bool {
    if origin.is_empty() {
        return false;
    }
    let Ok(context) = cef_request_context_for_profile(profile) else {
        return false;
    };
    let origin = CefString::from(origin);
    for content_type in content_types {
        context.set_content_setting(Some(&origin), Some(&origin), *content_type, value);
    }
    true
}

/// Clears a site's Allow or Don't Allow in a browser profile, so the site asks again. False when
/// the profile's browser context could not be reached.
pub fn forget_local_network_access_answer(profile: &str, origin: &str) -> bool {
    set_local_network_access_setting(
        profile,
        origin,
        &LOCAL_NETWORK_ACCESS_CONTENT_TYPES,
        ContentSettingValues::DEFAULT,
    )
}

/// The Chromium content settings a Local Network Access prompt asks about.
pub(crate) fn local_network_access_content_types(
    loopback_network: bool,
    local_network: bool,
    local_network_deprecated: bool,
) -> Vec<ContentSettingTypes> {
    let mut content_types = Vec::new();
    if loopback_network {
        content_types.push(ContentSettingTypes::LOOPBACK_NETWORK);
    }
    if local_network {
        content_types.push(ContentSettingTypes::LOCAL_NETWORK);
    }
    if local_network_deprecated {
        content_types.push(ContentSettingTypes::LOCAL_NETWORK_ACCESS);
    }
    content_types
}

struct PendingSitePrompt {
    browser_id: i32,
    /// Chromium's id for a Local Network Access prompt, which Chromium dismisses itself.
    chromium_prompt_id: Option<u64>,
    /// An app-link question ends when its page's main frame leaves this origin.
    page_origin: Option<String>,
    page_gone: futures::channel::oneshot::Sender<()>,
}

thread_local! {
    /// Questions the app is showing for a page, by `SitePromptPageWatch` id: the signal that takes
    /// the app's question down when the page goes away.
    static PENDING_SITE_PROMPTS: RefCell<HashMap<u64, PendingSitePrompt>> =
        RefCell::new(HashMap::new());
    static NEXT_SITE_PROMPT_ID: Cell<u64> = const { Cell::new(1) };
}

/// CDXC:Browser 2026-10-10 WHY:
/// The app's "Allow … to connect to apps on this computer?" question outlived its tab (Linear's
/// tab closed under it in the live test). Chromium dismisses a permission prompt when the page
/// navigates or closes (`on_dismiss_permission_prompt`), a closing browser ends every question it
/// raised, and an app-link question ends when its page leaves the asking site, so the app's
/// question follows its page. Dropping the watch (the request answered or let go) unregisters it.
pub(crate) struct SitePromptPageWatch {
    id: u64,
    page_gone: Option<futures::channel::oneshot::Receiver<()>>,
}

impl SitePromptPageWatch {
    /// Runs on the CEF UI thread, which owns the registry.
    pub(crate) fn register(
        browser_id: i32,
        chromium_prompt_id: Option<u64>,
        page_origin: Option<String>,
    ) -> Self {
        let id = NEXT_SITE_PROMPT_ID.with(|next| next.replace(next.get() + 1));
        let (sender, receiver) = futures::channel::oneshot::channel();
        PENDING_SITE_PROMPTS.with(|pending| {
            pending.borrow_mut().insert(
                id,
                PendingSitePrompt {
                    browser_id,
                    chromium_prompt_id,
                    page_origin,
                    page_gone: sender,
                },
            )
        });
        Self {
            id,
            page_gone: Some(receiver),
        }
    }

    fn page_gone(&mut self) -> impl std::future::Future<Output = ()> + 'static {
        let page_gone = self.page_gone.take();
        async move {
            let gone = match page_gone {
                Some(page_gone) => page_gone.await.is_ok(),
                None => false,
            };
            if !gone {
                futures::future::pending::<()>().await;
            }
        }
    }
}

impl Drop for SitePromptPageWatch {
    fn drop(&mut self) {
        PENDING_SITE_PROMPTS.with(|pending| pending.borrow_mut().remove(&self.id));
    }
}

/// Ends the app's questions whose page matches, telling each that its page is gone.
fn end_site_prompts(matches: impl Fn(&PendingSitePrompt) -> bool) {
    let ended = PENDING_SITE_PROMPTS.with(|pending| {
        let mut pending = pending.borrow_mut();
        let ids = pending
            .iter()
            .filter(|(_, prompt)| matches(prompt))
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        ids.into_iter()
            .filter_map(|id| pending.remove(&id))
            .collect::<Vec<_>>()
    });
    for prompt in ended {
        let _ = prompt.page_gone.send(());
    }
}

pub(crate) fn local_network_prompt_dismissed(prompt_id: u64) {
    end_site_prompts(|prompt| prompt.chromium_prompt_id == Some(prompt_id));
}

pub(crate) fn site_prompts_browser_closed(browser_id: i32) {
    end_site_prompts(|prompt| prompt.browser_id == browser_id);
}

/// A main-frame navigation to `origin` ends the browser's app-link questions from another site.
fn site_prompts_page_navigated(browser_id: i32, origin: &str) {
    end_site_prompts(|prompt| {
        prompt.browser_id == browser_id
            && prompt
                .page_origin
                .as_deref()
                .is_some_and(|asked_from| asked_from != origin)
    });
}

pub type BrowserSiteRequestHandler = StdRc<dyn Fn(BrowserSiteRequest)>;

thread_local! {
    static BROWSER_SITE_REQUEST_HANDLER: RefCell<Option<BrowserSiteRequestHandler>> =
        const { RefCell::new(None) };
}

/// The app registers this once; it runs on the CEF UI thread, which is the app's main thread.
pub fn set_browser_site_request_handler(handler: BrowserSiteRequestHandler) {
    BROWSER_SITE_REQUEST_HANDLER.with(|slot| *slot.borrow_mut() = Some(handler));
}

pub(crate) fn dispatch_browser_site_request(request: BrowserSiteRequest) {
    let handler = BROWSER_SITE_REQUEST_HANDLER.with(|slot| slot.borrow().clone());
    if let Some(handler) = handler {
        handler(request);
    }
}

/// Schemes no page may hand to the OS: the ones Chromium loads itself, Ghostex's own (a page must
/// not drive this app through the OS), and Chrome's never-launch list plus two Windows exploit
/// vectors (`ms-msdt`, `search-ms`).
const NEVER_OPENED_IN_ANOTHER_APP_SCHEMES: &[&str] = &[
    "about",
    "afp",
    "blob",
    "cef",
    "chrome",
    "chrome-devtools",
    "chrome-error",
    "chrome-extension",
    "chrome-search",
    "chrome-untrusted",
    "data",
    "devtools",
    "disk",
    "disks",
    "file",
    "filesystem",
    "ftp",
    "ghostex",
    "hcp",
    "http",
    "https",
    "ie.http",
    "javascript",
    "ms-help",
    "ms-msdt",
    "nntp",
    "res",
    "search-ms",
    "shell",
    "vbscript",
    "view-source",
    "vnd.ms.radio",
    "ws",
    "wss",
];

/// The scheme of a link that only another app can open, or None when the page must not hand it to
/// the OS at all.
pub(crate) fn external_app_link_scheme(url: &str) -> Option<String> {
    let (scheme, _) = url.trim().split_once(':')?;
    let mut chars = scheme.chars();
    let valid = chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    // A one-letter "scheme" is a Windows drive (`C:\…`), not a link.
    if !valid || scheme.len() < 2 {
        return None;
    }
    let scheme = scheme.to_ascii_lowercase();
    (!NEVER_OPENED_IN_ANOTHER_APP_SCHEMES.contains(&scheme.as_str())).then_some(scheme)
}

fn web_page_origin(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .map(|url| url.origin().ascii_serialization())
        .unwrap_or_default()
}

/// The page the request came from: the browser's main frame (the Okta link loads in a hidden
/// iframe, whose own URL says nothing).
fn browser_page_origin(browser: Option<&mut cef::Browser>) -> String {
    browser
        .and_then(|browser| browser.main_frame())
        .map(|frame| web_page_origin(&CefString::from(&frame.url()).to_string()))
        .unwrap_or_default()
}

/// For the popup paths (`window.open`, target=_blank, Cmd/Ctrl-click), which run on the UI thread:
/// an app link is handed to the app instead of becoming a blank Browser tab. True when it was one.
pub(crate) fn dispatch_external_app_popup(
    browser: Option<&mut cef::Browser>,
    target_url: Option<&CefString>,
) -> bool {
    let url = target_url.map(CefString::to_string).unwrap_or_default();
    let Some(scheme) = external_app_link_scheme(&url) else {
        return false;
    };
    let browser_id = browser
        .as_deref()
        .map(|browser| browser.identifier())
        .unwrap_or_default();
    dispatch_browser_site_request(BrowserSiteRequest::OpenExternalApp(
        BrowserExternalAppRequest::new(url, scheme, browser_page_origin(browser), browser_id, None),
    ));
    true
}

/// CDXC:Browser 2026-10-10 WHY:
/// Handing an app link to the OS from `on_protocol_execution` still commits Chromium's
/// ERR_UNKNOWN_URL_SCHEME page in the frame (linear.app redirecting a Browser tab to `linear://`
/// when Linear's "Open in desktop app" is on), so every request handler's `on_before_browse` asks
/// here first: the navigation is cancelled, the frame keeps what it showed, and the app prompts as
/// before. A Browser tab that never committed a page was opened only for that link, so it closes
/// once the prompt is done with, as Chrome does (`page_metadata_handler` is Browser tabs only).
/// True when the navigation was such a link and must be cancelled.
pub(crate) fn cancel_external_app_navigation(
    browser: Option<&mut cef::Browser>,
    frame: Option<&mut Frame>,
    request: Option<&mut Request>,
    page_metadata_handler: Option<&BrowserPageMetadataHandler>,
) -> bool {
    let url = request
        .map(|request| CefString::from(&request.url()).to_string())
        .unwrap_or_default();
    let is_main_frame = frame.is_none_or(|frame| frame.is_main() != 0);
    let browser_id = browser
        .as_deref()
        .map(|browser| browser.identifier())
        .unwrap_or_default();
    let Some(scheme) = external_app_link_scheme(&url) else {
        if is_main_frame {
            site_prompts_page_navigated(browser_id, &web_page_origin(&url));
        }
        return false;
    };
    let committed_page = browser.as_deref().is_some_and(browser_committed_a_page);
    let close_link_only_tab = page_metadata_handler
        .filter(|_| is_main_frame && !committed_page)
        .cloned();
    dispatch_browser_site_request(BrowserSiteRequest::OpenExternalApp(
        BrowserExternalAppRequest::new(
            url,
            scheme,
            browser_page_origin(browser),
            browser_id,
            close_link_only_tab,
        ),
    ));
    true
}

/// CDXC:Browser 2026-10-10 WHY:
/// `Browser::has_document` was false when linear.app's script sent its already-shown page to
/// `linear://` (live test on Windows), so the Work page's Linear tab was closed under the user.
/// The main frame's URL is its last committed one (`about:blank` or empty before the first
/// commit) and stays put while the app-link navigation is pending, so it tells whether the tab
/// ever showed a page.
fn browser_committed_a_page(browser: &cef::Browser) -> bool {
    browser.main_frame().is_some_and(|frame| {
        let url = CefString::from(&frame.url()).to_string();
        let url = url.trim();
        !url.is_empty() && !url.eq_ignore_ascii_case("about:blank")
    })
}

wrap_task! {
    pub(crate) struct GhostexDispatchExternalAppRequest {
        url: String,
        scheme: String,
        origin: String,
        browser_id: i32,
    }

    impl Task {
        fn execute(&self) {
            dispatch_browser_site_request(BrowserSiteRequest::OpenExternalApp(
                BrowserExternalAppRequest::new(
                    self.url.clone(),
                    self.scheme.clone(),
                    self.origin.clone(),
                    self.browser_id,
                    None,
                ),
            ));
        }
    }
}

wrap_resource_request_handler! {
    pub(crate) struct GhostexExternalAppResourceRequestHandler;

    impl ResourceRequestHandler {
        // The cef-rs default answers RV_CANCEL (see GhostexManageDocsResourceRequestHandler).
        fn on_before_resource_load(
            &self,
            _browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            ReturnValue::CONTINUE
        }

        fn on_protocol_execution(
            &self,
            browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            allow_os_execution: Option<&mut c_int>,
        ) {
            if let Some(allow_os_execution) = allow_os_execution {
                *allow_os_execution = 0;
            }
            let Some(url) = request.map(|request| CefString::from(&request.url()).to_string())
            else {
                return;
            };
            let Some(scheme) = external_app_link_scheme(&url) else {
                return;
            };
            // This runs on CEF's IO thread; the app's handler lives on the UI thread.
            let browser_id = browser
                .as_deref()
                .map(|browser| browser.identifier())
                .unwrap_or_default();
            let mut task = GhostexDispatchExternalAppRequest::new(
                url,
                scheme,
                browser_page_origin(browser),
                browser_id,
            );
            post_task(ThreadId::UI, Some(&mut task));
        }
    }
}

/// The resource handler for a request only another app can open, so CEF calls
/// `on_protocol_execution` for it; every other request keeps CEF's default handling.
pub(crate) fn external_app_resource_request_handler(
    request: Option<&mut Request>,
) -> Option<ResourceRequestHandler> {
    let url = CefString::from(&request?.url()).to_string();
    external_app_link_scheme(&url).map(|_| GhostexExternalAppResourceRequestHandler::new())
}

// For pages with no request handler of their own (extension views, modals, panels).
wrap_request_handler! {
    pub(crate) struct GhostexGpuiExternalAppRequestHandler;

    impl RequestHandler {
        fn on_before_browse(
            &self,
            browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _user_gesture: c_int,
            _is_redirect: c_int,
        ) -> c_int {
            cancel_external_app_navigation(browser, frame, request, None) as c_int
        }

        fn resource_request_handler(
            &self,
            _browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _is_navigation: c_int,
            _is_download: c_int,
            _request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut c_int>,
        ) -> Option<ResourceRequestHandler> {
            external_app_resource_request_handler(request)
        }

        fn on_open_urlfrom_tab(
            &self,
            browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            target_url: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: c_int,
        ) -> c_int {
            dispatch_external_app_popup(browser, target_url) as c_int
        }
    }
}
