// C1 wave-3 re-cluster: the browser toolbar action, find state, and CEF media-permission prompt/decision types, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

#[derive(Clone, Copy)]
pub(crate) enum BrowserToolbarAction {
    Back,
    Forward,
    Reload,
    StopLoading,
    Home,
    FeedbackTool,
    ResetZoom,
    ResetMediaPermissions,
    HistoryMenu,
    ProfileMenu,
    DevTools,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct GpuiBrowserFindState {
    pub(crate) query: String,
    pub(crate) match_count: i32,
    pub(crate) active_match_ordinal: i32,
    pub(crate) final_update: bool,
}

/*
CDXC:GPUIBrowserMediaPermissions 2026-07-27:
Browser panes answer CEF microphone/camera requests with a real in-pane
permission prompt instead of the Alloy default (silent deny). The prompt is a
normal layout row between the toolbar and the page body — no overlay, no
hit-test routing — and the answer is remembered per browser profile + page
origin so a site asks once. Persistence stores only the scheme+authority
origin marker plus allow/block, matching the favicon-marker privacy rule: no
paths, query strings, fragments, credentials, or page content.

Requests arrive one at a time per tab but a page can ask for the microphone
and the camera in separate calls, so pending prompts queue per tab and the
front one renders. Dropping an unanswered prompt cancels its CEF request, so
closing the tab or navigating away releases the page's pending promise.
*/
pub(crate) struct GpuiBrowserMediaPermissionPrompt {
    pub(crate) profile_id: BrowserProfileId,
    pub(crate) origin: String,
    /// Everything the page asked for; the answer grants the allowed subset of
    /// this, including devices allowed by an earlier prompt.
    pub(crate) requested: cef::BrowserMediaAccessKinds,
    /// The undecided subset this prompt actually asks about, so an origin that
    /// already allowed the microphone is only asked about the camera.
    pub(crate) pending: cef::BrowserMediaAccessKinds,
    pub(crate) request: cef::BrowserMediaAccessRequest,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiBrowserMediaPermissionDecision {
    pub(crate) microphone: Option<bool>,
    pub(crate) camera: Option<bool>,
}

impl GpuiBrowserMediaPermissionDecision {
    pub(crate) fn is_empty(self) -> bool {
        self.microphone.is_none() && self.camera.is_none()
    }

    /// Requested devices this origin has no stored answer for yet.
    pub(crate) fn undecided(
        self,
        requested: cef::BrowserMediaAccessKinds,
    ) -> cef::BrowserMediaAccessKinds {
        cef::BrowserMediaAccessKinds {
            microphone: requested.microphone && self.microphone.is_none(),
            camera: requested.camera && self.camera.is_none(),
        }
    }

    /// Requested devices this origin is already allowed to use.
    pub(crate) fn granted(
        self,
        requested: cef::BrowserMediaAccessKinds,
    ) -> cef::BrowserMediaAccessKinds {
        cef::BrowserMediaAccessKinds {
            microphone: requested.microphone && self.microphone == Some(true),
            camera: requested.camera && self.camera == Some(true),
        }
    }

    pub(crate) fn record(&mut self, kinds: cef::BrowserMediaAccessKinds, allow: bool) {
        if kinds.microphone {
            self.microphone = Some(allow);
        }
        if kinds.camera {
            self.camera = Some(allow);
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct GpuiBrowserMediaPermissionDecisions {
    pub(crate) origins: HashMap<String, GpuiBrowserMediaPermissionDecision>,
}

impl GpuiBrowserMediaPermissionDecisions {
    pub(crate) fn decision(
        &self,
        profile_id: BrowserProfileId,
        origin: &str,
    ) -> GpuiBrowserMediaPermissionDecision {
        self.origins
            .get(&gpui_browser_media_permission_key(profile_id, origin))
            .copied()
            .unwrap_or_default()
    }

    pub(crate) fn record(
        &mut self,
        profile_id: BrowserProfileId,
        origin: &str,
        kinds: cef::BrowserMediaAccessKinds,
        allow: bool,
    ) {
        self.origins
            .entry(gpui_browser_media_permission_key(profile_id, origin))
            .or_default()
            .record(kinds, allow);
    }

    /// Returns true when a stored decision was actually removed, so callers
    /// only reload the page when the site will really be asked again.
    pub(crate) fn forget(&mut self, profile_id: BrowserProfileId, origin: &str) -> bool {
        self.origins
            .remove(&gpui_browser_media_permission_key(profile_id, origin))
            .is_some_and(|decision| !decision.is_empty())
    }
}

pub(crate) fn gpui_browser_media_permission_key(
    profile_id: BrowserProfileId,
    origin: &str,
) -> String {
    format!("{}|{origin}", profile_id.cef_profile_string())
}

/// Normalizes a CEF requesting origin down to the scheme+authority marker used
/// as the stored permission key. Opaque or authority-less origins (`null`,
/// `data:`, `about:`) have no stable identity to remember, so they get no key
/// and are never prompted for.
pub(crate) fn gpui_browser_media_permission_origin(raw: &str) -> Option<String> {
    let (scheme, rest) = raw.trim().split_once("://")?;
    let scheme = scheme.trim().to_ascii_lowercase();
    if scheme.is_empty() {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host)
        .to_ascii_lowercase();
    if authority.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{authority}"))
}

pub(crate) fn gpui_browser_media_permission_display_origin(origin: &str) -> String {
    origin
        .split_once("://")
        .map_or(origin, |(_, authority)| authority)
        .to_string()
}

pub(crate) fn gpui_browser_media_permission_kinds_label(
    kinds: cef::BrowserMediaAccessKinds,
) -> &'static str {
    match (kinds.microphone, kinds.camera) {
        (true, true) => "your microphone and camera",
        (true, false) => "your microphone",
        _ => "your camera",
    }
}

#[derive(Clone, Copy)]
pub(crate) enum GpuiFocusedSurfaceZoomCommand {
    In,
    Out,
    Reset,
}
