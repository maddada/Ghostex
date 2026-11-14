// C4 light split: Docs resource types/serving plus the
// ResourceHandler/ResourceRequestHandler/RequestHandler impls that plug into
// the CEF Client -- Docs static-file serving, sidebar renderer lifecycle
// telemetry, and browser popup link routing. Pure move out of `cef/shell.rs`.
// See docs/2026-08-22/repo-restructure/SPLITS.md C4.
use super::*;
use crate::app::helpers::manage_embed_page_rules::{
    manage_docs_page_may_read, manage_docs_resource_path_is_hidden, manage_embed_url_resource_path,
    manage_embed_url_shows_html, manage_html_frame_may_navigate, manage_html_page_may_request,
};

/*
CDXC:Docs 2026-07-14:
Manage renders authored HTML through srcdoc, whose default base is the bundled
manage.html file. Give only the Manage CEF client a synthetic HTTPS resource
origin so normal browser URL resolution can load sibling CSS, JavaScript,
images, CSS url() values, and module imports. The provider resolves files on
CEF's blocking-file thread, canonicalizes both ends, and serves only paths
inside the configured Docs roots; ordinary Browser/sidebar/workarea clients
never receive this request handler or the project path.
*/
pub(crate) const MANAGE_DOCS_RESOURCE_BASE_URL: &str =
    PROJECT_WORKAREA_MANAGE_DOCS_RESOURCE_BASE_URL;

/// The scope's types and file reads live in `app/helpers/manage_docs_resources.rs` so the native
/// Files view works without CEF; this module only serves them to the embed page.
pub use crate::app::helpers::manage_docs_resources::ManageDocsResourceScope;
pub(crate) use crate::app::helpers::manage_docs_resources::{
    ManageDocsResourceSource, resolve_manage_docs_local_resource,
};

impl ManageDocsResourceScope {
    pub fn base_url(&self) -> &'static str {
        MANAGE_DOCS_RESOURCE_BASE_URL
    }

    /// `page_url` is the embed page the surface opens, the one main-frame document it may show.
    pub(crate) fn request_handler(
        &self,
        page_url: &str,
        bridge_event_handler: Option<ProjectWorkareaBridgeEventHandler>,
    ) -> RequestHandler {
        GhostexManageDocsRequestHandler::new(
            self.source.clone(),
            first_party_page_entry_identity(page_url),
            bridge_event_handler,
        )
    }
}

/*
CDXC:Docs 2026-08-07:
Serve Docs resources straight from a CEF resource handler instead of the cef
wrapper's ResourceManager. That wrapper re-locks its own manager mutex while
already holding it (ResourceManager::send_request -> ResourceManagerRequest::
send_request), so the very first Docs subresource permanently wedged the
browser-process IO thread and froze every CEF pane in the app. We need no
provider ordering or async continuation here, so the direct handler is both
correct and simpler: CEF calls `open`/`read` on a blocking-capable worker
sequence, never the IO thread, which is exactly where the file open, the
remote fetch, and the reads belong.
*/
pub(crate) fn manage_docs_resource_relative_path(url: &str) -> Option<String> {
    let encoded_relative_path = url.strip_prefix(MANAGE_DOCS_RESOURCE_BASE_URL)?;
    let encoded_relative_path = get_url_without_query_or_fragment(encoded_relative_path);
    let relative_path = percent_decode_str(encoded_relative_path)
        .decode_utf8()
        .ok()?;
    if relative_path.is_empty()
        || relative_path.contains(['\0', '\\'])
        || relative_path.starts_with('/')
    {
        return None;
    }
    if relative_path
        .split('/')
        .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return None;
    }
    // CDXC:Docs 2026-11-14 DECISION: pages never load dot-files or anything in a dot-folder
    // (`.env`, `.git/`, `.claude/`), wherever they sit (app/helpers/manage_embed_page_rules.rs).
    if manage_docs_resource_path_is_hidden(&relative_path) {
        return None;
    }
    Some(relative_path.to_string())
}

/// Where a request in the Files embed page comes from, for the rules in
/// `app/helpers/manage_embed_page_rules.rs`.
pub(crate) enum ManageEmbedRequestSource {
    /// Ghostex's own embed page (the main frame), or a drawing's or a media file's frames.
    App,
    /// The rendered HTML file, or a frame or worker it made; carries the page's resource path.
    HtmlPage { resource_path: String },
}

/// Every frame below the main frame of an HTML embed is the rendered file; so is a request with
/// no frame (a worker) there. The main frame is Ghostex's own page.
pub(crate) fn manage_embed_request_source(
    browser: Option<&mut cef::Browser>,
    frame: Option<&mut Frame>,
) -> ManageEmbedRequestSource {
    if frame.as_ref().is_some_and(|frame| frame.is_main() != 0) {
        return ManageEmbedRequestSource::App;
    }
    let main_url = browser
        .and_then(|browser| browser.main_frame())
        .map(|frame| CefString::from(&frame.url()).to_string())
        .unwrap_or_default();
    if !manage_embed_url_shows_html(&main_url) {
        return ManageEmbedRequestSource::App;
    }
    ManageEmbedRequestSource::HtmlPage {
        resource_path: manage_embed_url_resource_path(&main_url).unwrap_or_default(),
    }
}

/// Opens a Docs resource, from `range_header`'s first byte range when the request carries a
/// satisfiable one. Runs on a CEF worker sequence, never the IO thread.
pub(crate) fn open_manage_docs_resource(
    source: &ManageDocsResourceSource,
    relative_path: &str,
    range_header: Option<&str>,
) -> Option<ManageDocsResourceBody> {
    match source {
        ManageDocsResourceSource::Local {
            resolve_dynamic_root,
            resolve_root,
            resolved_root,
        } => {
            let candidate = resolve_manage_docs_local_resource(
                resolve_dynamic_root,
                resolve_root,
                resolved_root,
                relative_path,
            )?;
            let total = std::fs::metadata(&candidate)
                .ok()
                .map(|metadata| metadata.len());
            let file_name = candidate.to_string_lossy();
            let stream = stream_reader_create_for_file(Some(&CefString::from(file_name.as_ref())))?;
            let range = total.and_then(|total| manage_docs_byte_range(range_header?, total));
            // SEEK_SET; a stream that cannot seek answers the whole file instead.
            let range = range.filter(|(start, _)| stream.seek(*start as i64, 0) == 0);
            Some(ManageDocsResourceBody::new(
                ManageDocsResourceBytes::Stream(stream),
                total,
                range,
            ))
        }
        ManageDocsResourceSource::Remote { loader } => {
            let data = loader(relative_path)?;
            let total = data.len() as u64;
            let range = range_header.and_then(|header| manage_docs_byte_range(header, total));
            let offset = range.map(|(start, _)| start as usize).unwrap_or(0);
            Some(ManageDocsResourceBody::new(
                ManageDocsResourceBytes::Buffer { data, offset },
                Some(total),
                range,
            ))
        }
    }
}

/// The inclusive byte range a `Range: bytes=…` header asks of a `total`-byte resource: its first
/// range only, clamped to the end, `None` when it is malformed or starts past the end.
///
/// CDXC:Docs 2026-09-27 WHY:
/// The Files view plays video and audio in the embed page from this origin; a media element seeks by asking for byte ranges, and without a 206 answer it could only play from the start and would read a long video whole.
fn manage_docs_byte_range(header: &str, total: u64) -> Option<(u64, u64)> {
    let spec = header
        .trim()
        .strip_prefix("bytes=")?
        .split(',')
        .next()?
        .trim();
    let (start, end) = spec.split_once('-')?;
    if total == 0 {
        return None;
    }
    let (start, end) = match (start.trim(), end.trim()) {
        ("", suffix) => {
            let suffix = suffix.parse::<u64>().ok().filter(|suffix| *suffix > 0)?;
            (total.saturating_sub(suffix), total - 1)
        }
        (start, "") => (start.parse::<u64>().ok()?, total - 1),
        (start, end) => (
            start.parse::<u64>().ok()?,
            end.parse::<u64>().ok()?.min(total - 1),
        ),
    };
    (start <= end && start < total).then_some((start, end))
}

pub(crate) enum ManageDocsResourceBytes {
    /// Local files stream from disk so a large Docs asset is never buffered whole.
    Stream(StreamReader),
    Buffer {
        data: Vec<u8>,
        offset: usize,
    },
}

/// What one Docs resource response sends: the bytes, the resource's whole size when known, and
/// the byte range answered for a `Range` request.
pub(crate) struct ManageDocsResourceBody {
    bytes: ManageDocsResourceBytes,
    total: Option<u64>,
    range: Option<(u64, u64)>,
    remaining: u64,
}

impl ManageDocsResourceBody {
    fn new(bytes: ManageDocsResourceBytes, total: Option<u64>, range: Option<(u64, u64)>) -> Self {
        let remaining = match (range, total) {
            (Some((start, end)), _) => end - start + 1,
            (None, Some(total)) => total,
            (None, None) => u64::MAX,
        };
        Self {
            bytes,
            total,
            range,
            remaining,
        }
    }

    pub(crate) fn response_length(&self) -> i64 {
        match (self.range, self.total) {
            (Some(_), _) | (None, Some(_)) => self.remaining as i64,
            (None, None) => -1,
        }
    }

    /// `(start, end, total)` when this response answers a byte range.
    pub(crate) fn content_range(&self) -> Option<(u64, u64, u64)> {
        let (start, end) = self.range?;
        Some((start, end, self.total?))
    }

    pub(crate) fn read(&mut self, data_out: *mut u8, bytes_to_read: usize) -> usize {
        let bytes_to_read =
            bytes_to_read.min(usize::try_from(self.remaining).unwrap_or(usize::MAX));
        if bytes_to_read == 0 {
            return 0;
        }
        let count = Self::read_bytes(&mut self.bytes, data_out, bytes_to_read);
        self.remaining = self.remaining.saturating_sub(count as u64);
        count
    }

    fn read_bytes(
        bytes: &mut ManageDocsResourceBytes,
        data_out: *mut u8,
        bytes_to_read: usize,
    ) -> usize {
        match bytes {
            ManageDocsResourceBytes::Stream(stream) => stream.read(data_out, 1, bytes_to_read),
            ManageDocsResourceBytes::Buffer { data, offset } => {
                let available = data.len().saturating_sub(*offset);
                let count = available.min(bytes_to_read);
                if count > 0 {
                    // `data_out` is CEF's buffer, guaranteed to hold `bytes_to_read`.
                    unsafe {
                        std::ptr::copy_nonoverlapping(data.as_ptr().add(*offset), data_out, count);
                    }
                    *offset += count;
                }
                count
            }
        }
    }
}

wrap_resource_handler! {
    pub(crate) struct GhostexManageDocsResourceHandler {
        source: ManageDocsResourceSource,
        relative_path: String,
        range_header: Option<String>,
        // Whether the requesting document may read the answer (`Access-Control-Allow-Origin`).
        cors: bool,
        body: Arc<Mutex<Option<ManageDocsResourceBody>>>,
    }

    impl ResourceHandler {
        fn open(
            &self,
            _request: Option<&mut Request>,
            handle_request: Option<&mut c_int>,
            _callback: Option<&mut Callback>,
        ) -> c_int {
            // Handled synchronously on this worker sequence; blocking here is
            // the documented contract for `open`, unlike the IO thread. The
            // file open and the remote fetch below both depend on that.
            if let Some(handle_request) = handle_request {
                *handle_request = 1;
            }
            let Some(opened) = open_manage_docs_resource(
                &self.source,
                &self.relative_path,
                self.range_header.as_deref(),
            ) else {
                // Outside the Docs roots or unreadable: cancel the request.
                return 0;
            };
            let Ok(mut body) = self.body.lock() else {
                return 0;
            };
            *body = Some(opened);
            1
        }

        fn response_headers(
            &self,
            response: Option<&mut Response>,
            response_length: Option<&mut i64>,
            _redirect_url: Option<&mut CefString>,
        ) {
            let Some(response) = response else {
                return;
            };
            let content_range = self
                .body
                .lock()
                .ok()
                .and_then(|body| body.as_ref().and_then(ManageDocsResourceBody::content_range));
            if content_range.is_some() {
                response.set_status(206);
                response.set_status_text(Some(&CefString::from("Partial Content")));
            } else {
                response.set_status(200);
                response.set_status_text(Some(&CefString::from("OK")));
            }
            response.set_mime_type(Some(&CefString::from(
                get_mime_type(&self.relative_path).as_str(),
            )));
            let mut headers = string_multimap_alloc();
            if let Some(headers) = headers.as_mut() {
                /*
                CDXC:Docs 2026-11-14 DECISION:
                Only a document allowed to read a file gets the header that lets its script read
                the answer: Ghostex's own embed page, a font (it loads in CORS mode and shows
                nothing of its bytes), and a rendered HTML file reading inside its own folder or
                below. Pictures, stylesheets and scripts beside it still load without it.
                */
                if self.cors {
                    string_multimap_append(
                        Some(headers),
                        Some(&CefString::from("Access-Control-Allow-Origin")),
                        Some(&CefString::from("*")),
                    );
                }
                string_multimap_append(
                    Some(headers),
                    Some(&CefString::from("Cache-Control")),
                    Some(&CefString::from("no-store")),
                );
                string_multimap_append(
                    Some(headers),
                    Some(&CefString::from("Accept-Ranges")),
                    Some(&CefString::from("bytes")),
                );
                if let Some((start, end, total)) = content_range {
                    string_multimap_append(
                        Some(headers),
                        Some(&CefString::from("Content-Range")),
                        Some(&CefString::from(format!("bytes {start}-{end}/{total}").as_str())),
                    );
                }
                response.set_header_map(Some(headers));
            }
            if let Some(response_length) = response_length {
                *response_length = self
                    .body
                    .lock()
                    .ok()
                    .and_then(|body| body.as_ref().map(ManageDocsResourceBody::response_length))
                    .unwrap_or(-1);
            }
        }

        #[allow(clippy::not_unsafe_ptr_arg_deref)]
        fn read(
            &self,
            data_out: *mut u8,
            bytes_to_read: c_int,
            bytes_read: Option<&mut c_int>,
            _callback: Option<&mut ResourceReadCallback>,
        ) -> c_int {
            if bytes_to_read < 1 {
                return 0;
            }
            let Some(bytes_read) = bytes_read else {
                return 0;
            };
            let Ok(mut body) = self.body.lock() else {
                return 0;
            };
            let Some(body) = body.as_mut() else {
                *bytes_read = 0;
                return 0;
            };

            // Fill the buffer until it is full or the source reports EOF.
            *bytes_read = 0;
            loop {
                let data_out = unsafe { data_out.add(*bytes_read as usize) };
                let read = body.read(data_out, (bytes_to_read - *bytes_read) as usize);
                *bytes_read += read as c_int;
                if read == 0 || *bytes_read >= bytes_to_read {
                    break;
                }
            }

            // Returning 0 with no bytes read signals the end of the response.
            if *bytes_read > 0 { 1 } else { 0 }
        }
    }
}

wrap_resource_request_handler! {
    pub(crate) struct GhostexManageDocsResourceRequestHandler {
        source: ManageDocsResourceSource,
    }

    impl ResourceRequestHandler {
        /*
        CDXC:Docs 2026-08-08:
        CEF consults on_before_resource_load BEFORE resource_handler, and the
        generated cef-rs binding's inherited default returns
        ReturnValue::default() == RV_CANCEL. Without this explicit CONTINUE
        override, every Docs subresource request was aborted
        (net::ERR_ABORTED, canceled) before the resource handler was ever
        queried, so no image/CSS/JS in rendered HTML Docs could load.
        */
        fn on_before_resource_load(
            &self,
            _browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            ReturnValue::CONTINUE
        }

        fn resource_handler(
            &self,
            browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
        ) -> Option<ResourceHandler> {
            let request = request?;
            let request_url = CefString::from(&request.url()).to_string();
            let relative_path = manage_docs_resource_relative_path(&request_url)?;
            let range_header = CefString::from(&request.header_by_name(Some(&CefString::from("Range"))))
                .to_string();
            let cors = match manage_embed_request_source(browser, frame) {
                ManageEmbedRequestSource::App => true,
                ManageEmbedRequestSource::HtmlPage { resource_path } => {
                    request.resource_type() == cef::ResourceType::FONT_RESOURCE
                        || manage_docs_page_may_read(&resource_path, &relative_path)
                }
            };
            Some(GhostexManageDocsResourceHandler::new(
                self.source.clone(),
                relative_path,
                (!range_header.trim().is_empty()).then_some(range_header),
                cors,
                Arc::new(Mutex::new(None)),
            ))
        }
    }
}

wrap_request_handler! {
    pub(crate) struct GhostexGpuiBrowserRequestHandler {
        popup_open_handler: BrowserPopupOpenHandler,
        page_metadata_handler: Option<BrowserPageMetadataHandler>,
    }

    impl RequestHandler {
        fn on_before_browse(
            &self,
            browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _user_gesture: c_int,
            _is_redirect: c_int,
        ) -> c_int {
            cancel_external_app_navigation(
                browser,
                frame,
                request,
                self.page_metadata_handler.as_ref(),
            ) as c_int
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
            target_disposition: WindowOpenDisposition,
            _user_gesture: c_int,
        ) -> c_int {
            if dispatch_external_app_popup(browser, target_url) {
                return 1;
            }
            /*
            CDXC:Browser 2026-08-18:
            Chromium reports middle-click and Cmd/Ctrl-click link opens here,
            not through OnBeforePopup, so Browser panes need this callback to
            keep those gestures inside the GPUI Browser workspace. Forward only
            the requested target URL to the same shell tab model the popup path
            uses and return handled so Chromium creates no separate browser.
            Dispositions that are not a new browser (same-tab navigation,
            save-to-disk, ignored actions) stay on CEF's default path.

            Empty targets mirror the popup policy
            (CDXC:Browser 2026-06-23-11:43): handled here with no
            shell dispatch, because there is no transferable URL and no
            fallback transfer path.
            */
            let Some(placement) = browser_popup_placement_for_disposition(target_disposition) else {
                return 0;
            };
            if let Some(requested_url) = browser_popup_target_url_for_shell(target_url) {
                (self.popup_open_handler)(requested_url, placement);
            }
            1
        }
    }
}

/*
CDXC:SessionChat 2026-09-09 DECISION:
User: a first-party page (sidebar, session chat) must never navigate itself
away; if anything tries, the URL opens where a clicked link would go, in the
embedded Browser or the system browser depending on "Open links in embedded
browser". The chat pane has real browser history, so an un-intercepted link
in a transcript (a Storybook URL inside a tool-call block, 2026-09-09) used
to replace chat.html with that page until the user pressed Back. Only
main-frame navigations to a different document are refused; reloads and
query changes of the page's own entry, and every sub-frame load, pass.
The sidebar and chat are native now, so the Files embed page (an HTML file's
own links run in its sub-frame) is the first-party page this guards; extension
pages and website views keep their own navigation.
*/
pub(crate) fn first_party_page_entry_identity(url: &str) -> String {
    get_url_without_query_or_fragment(url).to_string()
}

/// Whether `url` shows the first-party page whose entry identity is `page`: the same address
/// without query or fragment, compared as parsed URLs so CEF's own spelling of a frame URL
/// (lower-case scheme and host, percent-encoding) still matches.
pub(crate) fn same_first_party_page(url: &str, page: &str) -> bool {
    let entry = first_party_page_entry_identity(url);
    match (url::Url::parse(&entry), url::Url::parse(page)) {
        (Ok(entry), Ok(page)) => {
            entry.scheme() == page.scheme()
                && entry.host_str() == page.host_str()
                && entry.port_or_known_default() == page.port_or_known_default()
                && entry.path() == page.path()
        }
        _ => entry == page,
    }
}

wrap_resource_request_handler! {
    // Cancels the one request it is given: something a rendered HTML file may not reach.
    pub(crate) struct GhostexRefusedResourceRequestHandler;

    impl ResourceRequestHandler {
        fn on_before_resource_load(
            &self,
            _browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            ReturnValue::CANCEL
        }
    }
}

wrap_request_handler! {
    pub(crate) struct GhostexManageDocsRequestHandler {
        source: ManageDocsResourceSource,
        entry_identity: String,
        bridge_event_handler: Option<ProjectWorkareaBridgeEventHandler>,
    }

    impl RequestHandler {
        fn on_before_browse(
            &self,
            browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            user_gesture: c_int,
            _is_redirect: c_int,
        ) -> c_int {
            let (mut browser, mut frame, mut request) = (browser, frame, request);
            // An HTML file's app links run in its sub-frame, so this comes before the frame check.
            if cancel_external_app_navigation(
                browser.as_deref_mut(),
                frame.as_deref_mut(),
                request.as_deref_mut(),
                None,
            ) {
                return 1;
            }
            let is_main_frame = frame.as_ref().map(|frame| frame.is_main() != 0).unwrap_or(true);
            if !is_main_frame {
                /*
                CDXC:Docs 2026-11-14 DECISION:
                A rendered HTML file's frames stay on their own document and the Files resource
                origin (User: network blocked for the page, so nothing it reads can leave). A link
                the user clicks still opens, where a clicked chat link would; a page cannot
                navigate anywhere by itself.
                */
                let ManageEmbedRequestSource::HtmlPage { .. } =
                    manage_embed_request_source(browser, frame)
                else {
                    return 0;
                };
                let request_url = request
                    .map(|request| CefString::from(&request.url()).to_string())
                    .unwrap_or_default();
                if manage_html_frame_may_navigate(&request_url) {
                    return 0;
                }
                if user_gesture != 0
                    && (request_url.starts_with("http://") || request_url.starts_with("https://"))
                    && let Some(handler) = self.bridge_event_handler.as_ref()
                {
                    handler(ProjectWorkareaBridgeEvent::RefusedPageNavigation(request_url));
                }
                return 1;
            }
            let Some(request_url) = request.map(|request| CefString::from(&request.url()).to_string())
            else {
                return 0;
            };
            if first_party_page_entry_identity(&request_url) == self.entry_identity {
                return 0;
            }
            if (request_url.starts_with("http://") || request_url.starts_with("https://"))
                && let Some(handler) = self.bridge_event_handler.as_ref()
            {
                handler(ProjectWorkareaBridgeEvent::RefusedPageNavigation(request_url));
            }
            1
        }

        fn resource_request_handler(
            &self,
            browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _is_navigation: c_int,
            _is_download: c_int,
            _request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut c_int>,
        ) -> Option<ResourceRequestHandler> {
            let Some(request) = request else {
                return None;
            };
            let request_url = CefString::from(&request.url()).to_string();
            if request_url.starts_with(MANAGE_DOCS_RESOURCE_BASE_URL) {
                return Some(GhostexManageDocsResourceRequestHandler::new(self.source.clone()));
            }
            // A rendered HTML file reaches nothing outside the Files resource origin but a few
            // library CDNs (CDXC:Docs 2026-11-14 in app/helpers/manage_embed_page_rules.rs).
            if let ManageEmbedRequestSource::HtmlPage { .. } =
                manage_embed_request_source(browser, frame)
            {
                let method = CefString::from(&request.method()).to_string();
                if !manage_html_page_may_request(&request_url, &method) {
                    return Some(GhostexRefusedResourceRequestHandler::new());
                }
            }
            external_app_resource_request_handler(Some(request))
        }
    }
}
