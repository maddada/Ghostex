use std::{
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use anyhow::Result;

use super::identity::{read_remote_access_identity, RemoteAccessIdentity};
use super::tailscale::{read_tailscale_status, TailscaleStatus};

/*
CDXC:RemotePairing 2026-09-03:
Settings → Remote polls `/api/remotePairingCode` every few seconds while it is
open, and both status handlers shell out (`scutil`, `tailscale status --json`)
to answer. The identity cannot change for the life of the process, so it is
read once; the Tailscale state can, so it is re-read after a short TTL that
is shared by every handler. The caches are process-wide statics because
there is exactly one probe target — this machine — per daemon.
*/
const TAILSCALE_STATUS_TTL: Duration = Duration::from_secs(10);

static IDENTITY: OnceLock<RemoteAccessIdentity> = OnceLock::new();
static TAILSCALE: Mutex<Option<(Instant, TailscaleStatus)>> = Mutex::new(None);

pub fn cached_remote_access_identity() -> Result<RemoteAccessIdentity> {
    if let Some(identity) = IDENTITY.get() {
        return Ok(identity.clone());
    }
    let identity = read_remote_access_identity()?;
    Ok(IDENTITY.get_or_init(|| identity).clone())
}

pub fn cached_tailscale_status() -> TailscaleStatus {
    let now = Instant::now();
    if let Some((read_at, status)) = TAILSCALE
        .lock()
        .expect("tailscale status cache lock")
        .as_ref()
    {
        if now.duration_since(*read_at) < TAILSCALE_STATUS_TTL {
            return status.clone();
        }
    }
    let status = read_tailscale_status();
    *TAILSCALE.lock().expect("tailscale status cache lock") = Some((now, status.clone()));
    status
}
