// C4 light split: Docs resource types/serving plus the
// ResourceHandler/ResourceRequestHandler/RequestHandler impls that plug into
// the CEF Client -- Docs static-file serving, sidebar renderer lifecycle
// telemetry, and browser popup link routing. Pure move out of `cef/shell.rs`.
// See docs/2026-08-22/repo-restructure/SPLITS.md C4.
use super::*;

/*
CDXC:GPUIManageHtmlResources 2026-07-14:
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

pub(crate) type ManageDocsRemoteResourceLoader = Arc<dyn Fn(&str) -> Option<Vec<u8>> + Send + Sync>;

/*
CDXC:DocsRootDirectory 2026-08-09:
The local Docs root is a configurable folder now, and resolving it reads the
project's Docs directory from the daemon. That resolution must not run on the
main thread while a CEF surface is being created, so the scope carries a
resolver that runs on the same blocking-capable worker sequence as the file
open, memoized so one document's images cost one lookup.

CDXC:DocsRootAdditive 2026-08-09: Docs serves TWO roots — the project's own and
the mounted Docs directory — so the resolver answers with every mount, each
carrying the path segment that addresses it and the relative roots a resource
may live under inside it. Which mounts exist and what they allow both come out
of the same daemon lookup, so neither can be answered before it.
*/
pub(crate) type ManageDocsLocalRootResolver =
    Arc<dyn Fn() -> Option<Vec<ManageDocsResourceRoot>> + Send + Sync>;
pub(crate) type ManageDocsDynamicRootResolver =
    Arc<dyn Fn() -> Option<ManageDocsResourceRoot> + Send + Sync>;

/// One mounted Docs root as the resource scope sees it: the reserved first path
/// segment that addresses it (empty for the project root, which owns bare
/// paths), the root itself, and the relative roots inside it a resource may
/// live under. An empty relative root means the whole tree.
#[derive(Clone)]
pub struct ManageDocsResourceRoot {
    pub allowed_relative_roots: Vec<String>,
    pub mount_segment: String,
    pub path: PathBuf,
}

#[derive(Clone)]
pub(crate) enum ManageDocsResourceSource {
    Local {
        resolve_dynamic_root: ManageDocsDynamicRootResolver,
        resolve_root: ManageDocsLocalRootResolver,
        /*
        CDXC:DocsRootDirectory 2026-08-10:
        Memoize only a successful resolution. The lookup reads the project's
        Docs directory from the daemon, so it can answer `None` for a reason
        that passes — daemon not reachable yet, project row not loaded. Sealing
        that first answer would leave every image and stylesheet in the document
        broken until the surface is recreated, with nothing shown to say why.
        */
        resolved_root: Arc<Mutex<Option<Vec<ManageDocsResourceRoot>>>>,
    },
    Remote {
        loader: ManageDocsRemoteResourceLoader,
    },
}

#[derive(Clone)]
pub struct ManageDocsResourceScope {
    pub(crate) source: ManageDocsResourceSource,
}

impl ManageDocsResourceScope {
    pub fn new(
        resolve_root: ManageDocsLocalRootResolver,
        resolve_dynamic_root: ManageDocsDynamicRootResolver,
    ) -> Self {
        Self {
            source: ManageDocsResourceSource::Local {
                resolve_dynamic_root,
                resolve_root,
                resolved_root: Arc::new(Mutex::new(None)),
            },
        }
    }

    pub fn new_remote(loader: ManageDocsRemoteResourceLoader) -> Self {
        Self {
            source: ManageDocsResourceSource::Remote { loader },
        }
    }

    pub fn base_url(&self) -> &'static str {
        MANAGE_DOCS_RESOURCE_BASE_URL
    }

    pub(crate) fn request_handler(&self) -> RequestHandler {
        GhostexManageDocsRequestHandler::new(self.source.clone())
    }
}

/*
CDXC:GPUIManageHtmlResources 2026-08-07:
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
    Some(relative_path.to_string())
}

/// Opens a Docs resource. Runs on a CEF worker sequence, never the IO thread.
pub(crate) fn open_manage_docs_resource(
    source: &ManageDocsResourceSource,
    relative_path: &str,
) -> Option<ManageDocsResourceBody> {
    match source {
        ManageDocsResourceSource::Local {
            resolve_dynamic_root,
            resolve_root,
            resolved_root,
        } => {
            /*
            CDXC:DocsRootAdditive 2026-08-09:
            The requested path names its own root through the reserved mount
            segment, exactly as the Docs bridge routes it, so an image beside a
            note in the mounted Docs directory resolves there and a path can
            never be resolved against the root it did not name.
            */
            let mut mounts = {
                let mut resolved = resolved_root.lock().ok()?;
                if resolved.is_none() {
                    *resolved = resolve_root();
                }
                resolved.clone()?
            };
            if let Some(dynamic_root) = resolve_dynamic_root() {
                mounts.retain(|mount| mount.mount_segment != dynamic_root.mount_segment);
                mounts.push(dynamic_root);
            }
            // A named mount claims its own segment first; the project root owns
            // every path no mount claimed.
            let (mount, relative_path) = mounts
                .iter()
                .filter(|mount| !mount.mount_segment.is_empty())
                .find_map(|mount| {
                    relative_path
                        .strip_prefix(&format!("{}/", mount.mount_segment))
                        .map(|inner| (mount, inner))
                })
                .or_else(|| {
                    mounts
                        .iter()
                        .find(|mount| mount.mount_segment.is_empty())
                        .map(|mount| (mount, relative_path))
                })?;
            let root = std::fs::canonicalize(&mount.path).ok()?;
            let candidate = std::fs::canonicalize(
                relative_path
                    .split('/')
                    .fold(root.clone(), |path, component| path.join(component)),
            )
            .ok()?;
            if !candidate.is_file() || !candidate.starts_with(&root) {
                return None;
            }
            let allowed = mount.allowed_relative_roots.iter().any(|relative_root| {
                let allowed_root = root.join(relative_root);
                std::fs::canonicalize(allowed_root)
                    .ok()
                    .is_some_and(|allowed_root| {
                        allowed_root.starts_with(&root) && candidate.starts_with(allowed_root)
                    })
            });
            if !allowed {
                return None;
            }
            let file_name = candidate.to_string_lossy();
            let stream = stream_reader_create_for_file(Some(&CefString::from(file_name.as_ref())))?;
            Some(ManageDocsResourceBody::Stream(stream))
        }
        ManageDocsResourceSource::Remote { loader } => {
            let data = loader(relative_path)?;
            Some(ManageDocsResourceBody::Buffer { data, offset: 0 })
        }
    }
}

pub(crate) enum ManageDocsResourceBody {
    /// Local files stream from disk so a large Docs asset is never buffered whole.
    Stream(StreamReader),
    Buffer {
        data: Vec<u8>,
        offset: usize,
    },
}

impl ManageDocsResourceBody {
    pub(crate) fn response_length(&self) -> i64 {
        match self {
            Self::Stream(_) => -1,
            Self::Buffer { data, .. } => data.len() as i64,
        }
    }

    pub(crate) fn read(&mut self, data_out: *mut u8, bytes_to_read: usize) -> usize {
        match self {
            Self::Stream(stream) => stream.read(data_out, 1, bytes_to_read),
            Self::Buffer { data, offset } => {
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
            let Some(opened) = open_manage_docs_resource(&self.source, &self.relative_path) else {
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
            response.set_status(200);
            response.set_status_text(Some(&CefString::from("OK")));
            response.set_mime_type(Some(&CefString::from(
                get_mime_type(&self.relative_path).as_str(),
            )));
            let mut headers = string_multimap_alloc();
            if let Some(headers) = headers.as_mut() {
                string_multimap_append(
                    Some(headers),
                    Some(&CefString::from("Access-Control-Allow-Origin")),
                    Some(&CefString::from("*")),
                );
                string_multimap_append(
                    Some(headers),
                    Some(&CefString::from("Cache-Control")),
                    Some(&CefString::from("no-store")),
                );
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
        CDXC:GPUIManageHtmlResources 2026-08-08:
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
            _browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
        ) -> Option<ResourceHandler> {
            let request_url = CefString::from(&request?.url()).to_string();
            let relative_path = manage_docs_resource_relative_path(&request_url)?;
            Some(GhostexManageDocsResourceHandler::new(
                self.source.clone(),
                relative_path,
                Arc::new(Mutex::new(None)),
            ))
        }
    }
}

wrap_request_handler! {
    pub(crate) struct GhostexGpuiSidebarRendererRequestHandler;

    impl RequestHandler {
        fn on_render_view_ready(&self, browser: Option<&mut cef::Browser>) {
            append_sidebar_renderer_lifecycle(
                "gpui.sidebar.rendererReady",
                browser,
                serde_json::json!({}),
            );
        }

        fn on_render_process_unresponsive(
            &self,
            browser: Option<&mut cef::Browser>,
            _callback: Option<&mut UnresponsiveProcessCallback>,
        ) -> c_int {
            append_sidebar_renderer_lifecycle(
                "gpui.sidebar.rendererUnresponsive",
                browser,
                serde_json::json!({}),
            );
            0
        }

        fn on_render_process_responsive(&self, browser: Option<&mut cef::Browser>) {
            append_sidebar_renderer_lifecycle(
                "gpui.sidebar.rendererResponsive",
                browser,
                serde_json::json!({}),
            );
        }

        fn on_render_process_terminated(
            &self,
            browser: Option<&mut cef::Browser>,
            status: TerminationStatus,
            error_code: c_int,
            error_string: Option<&CefString>,
        ) {
            append_sidebar_renderer_lifecycle(
                "gpui.sidebar.rendererTerminated",
                browser,
                serde_json::json!({
                    "cefCode": error_code,
                    "cefText": error_string.map(CefString::to_string),
                    "terminationKind": cef_termination_kind(status),
                    "terminationRaw": status.get_raw(),
                }),
            );
        }
    }
}

pub(crate) fn cef_termination_kind(status: TerminationStatus) -> &'static str {
    match status {
        TerminationStatus::ABNORMAL_TERMINATION => "abnormalTermination",
        TerminationStatus::PROCESS_WAS_KILLED => "processWasKilled",
        TerminationStatus::PROCESS_CRASHED => "processCrashed",
        TerminationStatus::PROCESS_OOM => "processOutOfMemory",
        TerminationStatus::LAUNCH_FAILED => "launchFailed",
        TerminationStatus::INTEGRITY_FAILURE => "integrityFailure",
        _ => "unknown",
    }
}

pub(crate) fn append_sidebar_renderer_lifecycle(
    event: &str,
    browser: Option<&mut cef::Browser>,
    mut details: serde_json::Value,
) {
    if let Some(details) = details.as_object_mut() {
        details.insert(
            "browserId".to_string(),
            browser
                .map(|browser| serde_json::Value::from(browser.identifier()))
                .unwrap_or(serde_json::Value::Null),
        );
        details.insert(
            "cefContextInitialized".to_string(),
            serde_json::Value::Bool(CEF_CONTEXT_INITIALIZED.load(Ordering::Acquire)),
        );
        details.insert(
            "runtimeShutdownStarted".to_string(),
            serde_json::Value::Bool(CEF_SHUTDOWN_IN_PROGRESS.load(Ordering::Acquire)),
        );
    }
    support_logs::append(GpuiSupportLog::SidebarRenderer, event, details);
}

wrap_request_handler! {
    pub(crate) struct GhostexGpuiBrowserRequestHandler {
        popup_open_handler: BrowserPopupOpenHandler,
    }

    impl RequestHandler {
        fn on_open_urlfrom_tab(
            &self,
            _browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            target_url: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            _user_gesture: c_int,
        ) -> c_int {
            /*
            CDXC:GPUIBrowserLinkNewTab 2026-08-18:
            Chromium reports middle-click and Cmd/Ctrl-click link opens here,
            not through OnBeforePopup, so Browser panes need this callback to
            keep those gestures inside the GPUI Browser workspace. Forward only
            the requested target URL to the same shell tab model the popup path
            uses and return handled so Chromium creates no separate browser.
            Dispositions that are not a new browser (same-tab navigation,
            save-to-disk, ignored actions) stay on CEF's default path.

            Empty targets mirror the popup policy
            (CDXC:GPUIBrowserPopups 2026-06-23-11:43): handled here with no
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

wrap_request_handler! {
    pub(crate) struct GhostexManageDocsRequestHandler {
        source: ManageDocsResourceSource,
    }

    impl RequestHandler {
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
            let request_url = request
                .map(|request| CefString::from(&request.url()).to_string())
                .unwrap_or_default();
            request_url.starts_with(MANAGE_DOCS_RESOURCE_BASE_URL).then(|| {
                GhostexManageDocsResourceRequestHandler::new(self.source.clone())
            })
        }
    }
}
