//! Every other permission a web page asks for (notifications, location, clipboard, camera and
//! microphone prompts, MIDI, fonts, window placement and the like): handed to the app, which asks
//! on the page the way it asks for Local Network Access (`app/browser_site_requests.rs`).
//!
//! CDXC:Browser 2026-10-23 WHY:
//! Alloy-style CEF answers a permission prompt it has no handler for with IGNORE, which leaves the page's request unanswered: claude.ai's "Enable notifications" spun forever because `Notification.requestPermission()` never resolved (seen 2026-10-23). Every prompt is now asked in the app with Allow and Block, kept per site in the asking browser profile as Chrome keeps it (so the site is not asked again), and a request that is not answered (the ×, Escape, the page gone, a page with no place to ask) is denied, so a page's promise always settles.

use super::*;

/// One thing a prompt asks for: how the question names it, and the Chromium content setting that
/// keeps the answer (none for the rare kinds Chromium keeps no setting for).
#[derive(Clone, Copy)]
pub struct SitePermissionKind {
    pub phrase: &'static str,
    pub(crate) content_type: Option<ContentSettingTypes>,
    /// Shown with the bell rather than the lock.
    pub notifications: bool,
}

const fn kind(
    phrase: &'static str,
    content_type: Option<ContentSettingTypes>,
) -> SitePermissionKind {
    SitePermissionKind {
        phrase,
        content_type,
        notifications: false,
    }
}

/// The kinds a prompt's permission bits ask for, in the order the question names them.
pub(crate) fn site_permission_kinds(requested: u32) -> Vec<SitePermissionKind> {
    let table: [(PermissionRequestTypes, SitePermissionKind); 20] = [
        (
            PermissionRequestTypes::NOTIFICATIONS,
            SitePermissionKind {
                phrase: "show notifications",
                content_type: Some(ContentSettingTypes::NOTIFICATIONS),
                notifications: true,
            },
        ),
        (
            PermissionRequestTypes::GEOLOCATION,
            kind("know your location", Some(ContentSettingTypes::GEOLOCATION)),
        ),
        (
            PermissionRequestTypes::CAMERA_STREAM,
            kind(
                "use your camera",
                Some(ContentSettingTypes::MEDIASTREAM_CAMERA),
            ),
        ),
        (
            PermissionRequestTypes::MIC_STREAM,
            kind(
                "use your microphone",
                Some(ContentSettingTypes::MEDIASTREAM_MIC),
            ),
        ),
        (
            PermissionRequestTypes::CAMERA_PAN_TILT_ZOOM,
            kind(
                "move your camera",
                Some(ContentSettingTypes::CAMERA_PAN_TILT_ZOOM),
            ),
        ),
        (
            PermissionRequestTypes::CLIPBOARD,
            kind(
                "see text and images you copy",
                Some(ContentSettingTypes::CLIPBOARD_READ_WRITE),
            ),
        ),
        (
            PermissionRequestTypes::MIDI_SYSEX,
            kind(
                "control your MIDI devices",
                Some(ContentSettingTypes::MIDI_SYSEX),
            ),
        ),
        (
            PermissionRequestTypes::IDLE_DETECTION,
            kind(
                "know when you are using this computer",
                Some(ContentSettingTypes::IDLE_DETECTION),
            ),
        ),
        (
            PermissionRequestTypes::LOCAL_FONTS,
            kind(
                "use the fonts on this computer",
                Some(ContentSettingTypes::LOCAL_FONTS),
            ),
        ),
        (
            PermissionRequestTypes::WINDOW_MANAGEMENT,
            kind(
                "manage windows on your displays",
                Some(ContentSettingTypes::WINDOW_MANAGEMENT),
            ),
        ),
        (
            PermissionRequestTypes::STORAGE_ACCESS,
            kind("use its cookies on this site", None),
        ),
        (
            PermissionRequestTypes::TOP_LEVEL_STORAGE_ACCESS,
            kind("use its cookies on this site", None),
        ),
        (
            PermissionRequestTypes::MULTIPLE_DOWNLOADS,
            kind(
                "download several files",
                Some(ContentSettingTypes::AUTOMATIC_DOWNLOADS),
            ),
        ),
        (
            PermissionRequestTypes::POINTER_LOCK,
            kind("lock your pointer", Some(ContentSettingTypes::POINTER_LOCK)),
        ),
        (
            PermissionRequestTypes::KEYBOARD_LOCK,
            kind(
                "capture your keyboard",
                Some(ContentSettingTypes::KEYBOARD_LOCK),
            ),
        ),
        (
            PermissionRequestTypes::PROTECTED_MEDIA_IDENTIFIER,
            kind("play protected content", None),
        ),
        (
            PermissionRequestTypes::REGISTER_PROTOCOL_HANDLER,
            kind("open a type of link", None),
        ),
        (
            PermissionRequestTypes::FILE_SYSTEM_ACCESS,
            kind("edit files on this computer", None),
        ),
        (
            PermissionRequestTypes::WEB_APP_INSTALLATION,
            kind("install itself as an app", None),
        ),
        (
            PermissionRequestTypes::VR_SESSION,
            kind("use virtual reality", None),
        ),
    ];
    let mut kinds: Vec<SitePermissionKind> = Vec::new();
    let mut known = 0u32;
    for (bit, kind) in table {
        let bit = bit.get_raw() as u32;
        known |= bit;
        if requested & bit != 0 && !kinds.iter().any(|seen| seen.phrase == kind.phrase) {
            kinds.push(kind);
        }
    }
    if requested & !known != 0 {
        kinds.push(SitePermissionKind {
            phrase: "use a feature of this computer",
            content_type: None,
            notifications: false,
        });
    }
    kinds
}

/// Whether a prompt asks only for the computer's fonts on behalf of one of the app's own pages
/// (an app-UI surface showing a page Ghostex serves on 127.0.0.1), which is allowed without asking.
///
/// CDXC:Browser 2026-10-25 WHY: code-server 4.143.1's terminal lists local fonts (xterm's ligatures addon calls `queryLocalFonts()`) as soon as a terminal opens, and since every prompt is asked on the page (CDXC:Browser 2026-10-23 above) the Code view showed "127.0.0.1:3777 wants to use the fonts on this computer". The app's own pages get the fonts silently; Browser panes and website views run in Browser profiles, so outside sites (and a user's own localhost pages there) are still asked, and an app-UI surface showing an outside HTTPS origin (a `server.url` extension) is still asked too.
pub(crate) fn own_page_local_fonts_request(profile: &str, origin: &str, requested: u32) -> bool {
    requested == PermissionRequestTypes::LOCAL_FONTS.get_raw() as u32
        && cef_profile_cache_segment(profile).is_some_and(cef_profile_is_app_ui)
        && cef_normalized_origin(origin).is_some_and(|origin| is_loopback_http_origin(&origin))
}

/// Every content setting a permission answer may have been kept under, for Forget all answers.
///
/// CDXC:Browser 2026-10-23 WHY: only plain per-site permission settings are written. Forget all answers once reset every kind a prompt can carry, including storage access, top-level storage access, protected media and protocol handlers, and Chromium aborted the whole app on one of them (a fatal check under `SetContentSetting`, every launch while the forget was pending, seen 2026-10-23). Those kinds are still asked and answered for the page, but not kept.
pub(crate) const SITE_PERMISSION_CONTENT_TYPES: [ContentSettingTypes; 13] = [
    ContentSettingTypes::NOTIFICATIONS,
    ContentSettingTypes::GEOLOCATION,
    ContentSettingTypes::MEDIASTREAM_CAMERA,
    ContentSettingTypes::MEDIASTREAM_MIC,
    ContentSettingTypes::CAMERA_PAN_TILT_ZOOM,
    ContentSettingTypes::CLIPBOARD_READ_WRITE,
    ContentSettingTypes::MIDI_SYSEX,
    ContentSettingTypes::IDLE_DETECTION,
    ContentSettingTypes::LOCAL_FONTS,
    ContentSettingTypes::WINDOW_MANAGEMENT,
    ContentSettingTypes::AUTOMATIC_DOWNLOADS,
    ContentSettingTypes::POINTER_LOCK,
    ContentSettingTypes::KEYBOARD_LOCK,
];

/// A pending permission prompt. Dropping it unanswered denies the request.
pub struct BrowserSitePermissionRequest {
    pub(crate) origin: String,
    pub(crate) profile: String,
    pub(crate) kinds: Vec<SitePermissionKind>,
    pub(crate) browser_id: i32,
    pub(crate) callback: Option<PermissionPromptCallback>,
    pub(crate) page_watch: SitePromptPageWatch,
}

impl BrowserSitePermissionRequest {
    pub fn origin(&self) -> &str {
        &self.origin
    }

    pub fn profile(&self) -> &str {
        &self.profile
    }

    pub fn kinds(&self) -> &[SitePermissionKind] {
        &self.kinds
    }

    pub fn browser_id(&self) -> i32 {
        self.browser_id
    }

    /// Resolves when the asking page is gone, so the app can take its question down.
    pub fn page_gone(&mut self) -> impl std::future::Future<Output = ()> + 'static {
        self.page_watch.page_gone()
    }

    /// Allow or Block, kept for the site in this browser profile.
    pub fn answer(mut self, allow: bool) {
        let value = if allow {
            ContentSettingValues::ALLOW
        } else {
            ContentSettingValues::BLOCK
        };
        if !self.origin.is_empty()
            && let Ok(context) = cef_request_context_for_profile(&self.profile)
        {
            let origin = CefString::from(self.origin.as_str());
            for content_type in self.kinds.iter().filter_map(|kind| kind.content_type) {
                context.set_content_setting(Some(&origin), Some(&origin), content_type, value);
            }
        }
        if let Some(callback) = self.callback.take() {
            callback.cont(if allow {
                PermissionRequestResult::ACCEPT
            } else {
                PermissionRequestResult::DENY
            });
        }
    }
}

impl Drop for BrowserSitePermissionRequest {
    fn drop(&mut self) {
        if let Some(callback) = self.callback.take() {
            callback.cont(PermissionRequestResult::DENY);
        }
    }
}

/// Whether the user allowed `page_url`'s site to show notifications in a browser profile, which a
/// page's notification must have before Ghostex shows it (the page could skip its own check).
pub fn site_may_notify(profile: &str, page_url: &str) -> bool {
    let Ok(context) = cef_request_context_for_profile(profile) else {
        return false;
    };
    let url = CefString::from(page_url);
    context.content_setting(Some(&url), Some(&url), ContentSettingTypes::NOTIFICATIONS)
        == ContentSettingValues::ALLOW
}

/// Clears a site's kept permission answers in a browser profile, so it asks again. False when the
/// profile's browser context could not be reached yet.
pub fn forget_site_permission_answers(profile: &str, origin: &str) -> bool {
    if origin.is_empty() {
        return true;
    }
    let Ok(context) = cef_request_context_for_profile(profile) else {
        return false;
    };
    let origin = CefString::from(origin);
    for content_type in SITE_PERMISSION_CONTENT_TYPES {
        context.set_content_setting(
            Some(&origin),
            Some(&origin),
            content_type,
            ContentSettingValues::DEFAULT,
        );
    }
    true
}
