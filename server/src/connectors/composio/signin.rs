//! Signing a workspace in to and out of Composio For You (Open Dot's `signIn`, `finishSignIn`,
//! `signOut`).

use crate::paths::GxserverPaths;

use super::account::{self, with_live, Connected};
use super::oauth_route::redirect_uri;
use super::tools;

/// The workspace is signed in to Composio For You (tokens saved).
pub(crate) fn signed_in(paths: &GxserverPaths, workspace: &str) -> bool {
    account::signed_in(paths, workspace)
}

/// The workspace's saved sign-in doesn't open any more (`unreadable` in `/api/readConnectors`).
pub(crate) fn sign_in_unreadable(paths: &GxserverPaths, workspace: &str) -> bool {
    account::sign_in_unreadable(paths, workspace)
}

/// Starts the workspace's sign-in: the authorization URL to open in the user's browser, or `None`
/// when already signed in. `redirect_base` is gxserver's own `http://127.0.0.1:<port>` origin.
pub(crate) fn sign_in(
    paths: &GxserverPaths,
    workspace: &str,
    redirect_base: &str,
) -> Result<Option<String>, String> {
    if with_live(paths, workspace, |live| live.session.is_some()) {
        return Ok(None);
    }
    match account::connect(paths, workspace, Some(&redirect_uri(redirect_base)), None)? {
        Connected::SignInAt(url) => Ok(Some(url)),
        Connected::Yes => {
            let listed = tools::refresh(paths, workspace);
            account::notify_changed(workspace);
            listed.map(|_| None)
        }
    }
}

/// The browser redirect: `state` must be the one the workspace's sign-in issued.
pub(super) fn finish_sign_in(
    paths: &GxserverPaths,
    workspace: &str,
    code: &str,
    state: &str,
) -> Result<(), String> {
    account::finish(paths, workspace, code, state)?;
    let listed = tools::refresh(paths, workspace);
    account::notify_changed(workspace);
    listed
}

pub(crate) fn sign_out(paths: &GxserverPaths, workspace: &str) {
    account::sign_out(paths, workspace);
    account::notify_changed(workspace);
}
