//! Each phone's Live Activity tokens on this computer, in a private file beside the push registrations.
//!
//! CDXC:Notifications 2026-11-11 WHY: The Live Activity tokens are send credentials like the Expo push token,
//! so they get the same care (folder 0700, file 0600, never in a log, a debug print or an answer), but their
//! own file: the push registrations file refuses fields it does not know, so adding them there would make an
//! older gxserver call the whole file damaged and stop every phone's notifications. An entry carries its own
//! pairing (`device_id`) and "Show session names" switch, so the banner works also for a phone whose
//! notifications are off; `/api/registerLiveActivity` (`registration.rs`) writes it, and an entry whose paired
//! phone was removed is never sent to.

use std::{
    fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, PoisonError},
};

use serde::{Deserialize, Serialize};

use crate::paths::GxserverPaths;
use crate::push::{is_printable_id, token_fingerprint, DEVICE_ID_MAX, MACHINE_ID_MAX};

const STORE_FILE_NAME: &str = "live-activity-registrations.json";
const STORE_TEMP_FILE_NAME: &str = "live-activity-registrations.json.tmp";
const STORE_VERSION: u32 = 1;
/// A device token in hex: 32 to 256 bytes (Live Activity and push-to-start tokens are longer than a
/// notification token). The relay checks the same.
const TOKEN_HEX_MIN: usize = 64;
const TOKEN_HEX_MAX: usize = 512;
/// The phone's other computers a registration may name.
pub(crate) const MAX_PHONE_COMPUTERS: usize = 32;

/// Which of Apple's services the phone's build talks to: production (TestFlight, App Store) or sandbox
/// (a development build).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum LiveActivityEnvironment {
    #[default]
    Production,
    Sandbox,
}

impl LiveActivityEnvironment {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Sandbox => "sandbox",
        }
    }
}

/// Which token of a phone a send used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// The running activity's update token.
    Activity,
    /// The app's push-to-start token (iOS 17.2+).
    PushToStart,
}

/// One phone's Live Activity tokens, keyed by the id the phone uses for this computer (the same id as its
/// push registration).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LiveActivityPhone {
    pub(crate) machine_id: String,
    /// The running activity's push token, hex; absent while the phone has no activity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) activity_token: Option<String>,
    /// The app's push-to-start token, hex; absent before iOS 17.2 or before iOS issued one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) push_to_start_token: Option<String>,
    #[serde(default)]
    pub(crate) environment: LiveActivityEnvironment,
    /// When the phone last sent these tokens (RFC 3339).
    pub(crate) seen_at: String,
    /// The phone's ids for its other paired computers, by their server ids, so a row of a connected
    /// computer opens that session on the phone; a connected computer the phone is not paired with has
    /// none, and its rows open the app.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) computers: Vec<PhoneComputer>,
    /// The paired device whose own token registered these (none for a caller with the computer's main
    /// token); only that phone may replace or clear them, and unpairing it ends them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) device_id: Option<String>,
    /// The phone's "Show session names" switch: on, a row leads with the session's title.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) session_names: bool,
}

/// One of the phone's other computers: its server id and the id the phone uses for it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PhoneComputer {
    pub(crate) server_id: String,
    pub(crate) machine_id: String,
}

impl LiveActivityPhone {
    /// The id this phone uses for the computer `server_id` (`this_server_id` is this computer).
    pub(crate) fn machine_id_for(&self, server_id: &str, this_server_id: &str) -> Option<String> {
        if server_id == this_server_id {
            return Some(self.machine_id.clone());
        }
        self.computers
            .iter()
            .find(|computer| computer.server_id == server_id)
            .map(|computer| computer.machine_id.clone())
    }
}

// The tokens are send credentials, so a debug print never shows them.
impl fmt::Debug for LiveActivityPhone {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let redacted = |token: &Option<String>| token.as_ref().map(|_| "<redacted>");
        formatter
            .debug_struct("LiveActivityPhone")
            .field("machine_id", &self.machine_id)
            .field("activity_token", &redacted(&self.activity_token))
            .field("push_to_start_token", &redacted(&self.push_to_start_token))
            .field("environment", &self.environment)
            .field("seen_at", &self.seen_at)
            .field("computers", &self.computers)
            .field("device_id", &self.device_id)
            .field("session_names", &self.session_names)
            .finish()
    }
}

/// A token in the form the relay takes: hex, 64 to 512 characters.
pub(crate) fn is_live_activity_token(value: &str) -> bool {
    (TOKEN_HEX_MIN..=TOKEN_HEX_MAX).contains(&value.len())
        && value.len() % 2 == 0
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// What the log calls a token: the first 8 hex of its SHA-256.
pub(crate) fn token_label(token: &str) -> String {
    token_fingerprint(token)[..8].to_string()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoreFile {
    version: u32,
    phones: Vec<LiveActivityPhone>,
}

#[derive(Serialize)]
struct StoreFileOut<'a> {
    version: u32,
    phones: &'a [LiveActivityPhone],
}

#[derive(Debug)]
pub(crate) enum LiveActivityStoreError {
    Unreadable(io::Error),
    Damaged,
    WriteFailed(io::Error),
}

impl fmt::Display for LiveActivityStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable(error) => write!(
                formatter,
                "The phones' Live Activity tokens in gxserver's auth folder could not be read ({error})."
            ),
            Self::Damaged => write!(
                formatter,
                "The phones' Live Activity tokens file {STORE_FILE_NAME} in gxserver's auth folder is damaged. Delete it; the phones send their tokens again."
            ),
            Self::WriteFailed(error) => write!(
                formatter,
                "The phones' Live Activity tokens could not be saved ({error}); the previous ones are unchanged."
            ),
        }
    }
}

impl std::error::Error for LiveActivityStoreError {}

static STORE_LOCK: Mutex<()> = Mutex::new(());

/// Holding a store handle holds the one process-wide lock; the sender never holds it during a request.
pub(crate) struct LiveActivityStore {
    directory: PathBuf,
    file: PathBuf,
    temp_file: PathBuf,
    _guard: MutexGuard<'static, ()>,
}

impl LiveActivityStore {
    pub(crate) fn open(paths: &GxserverPaths) -> Self {
        let guard = STORE_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        Self {
            directory: paths.auth_dir.clone(),
            file: paths.auth_dir.join(STORE_FILE_NAME),
            temp_file: paths.auth_dir.join(STORE_TEMP_FILE_NAME),
            _guard: guard,
        }
    }

    pub(crate) fn read(&self) -> Result<Vec<LiveActivityPhone>, LiveActivityStoreError> {
        let bytes = match fs::read(&self.file) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(LiveActivityStoreError::Unreadable(error)),
        };
        let store: StoreFile =
            serde_json::from_slice(&bytes).map_err(|_| LiveActivityStoreError::Damaged)?;
        if store.version != STORE_VERSION || !phones_are_valid(&store.phones) {
            return Err(LiveActivityStoreError::Damaged);
        }
        Ok(store.phones)
    }

    pub(crate) fn write(&self, phones: &[LiveActivityPhone]) -> Result<(), LiveActivityStoreError> {
        let bytes = serde_json::to_vec(&StoreFileOut {
            version: STORE_VERSION,
            phones,
        })
        .map_err(|error| LiveActivityStoreError::WriteFailed(error.into()))?;
        let result = self.replace_file(&bytes);
        if result.is_err() {
            let _ = fs::remove_file(&self.temp_file);
        }
        result.map_err(LiveActivityStoreError::WriteFailed)
    }

    fn replace_file(&self, bytes: &[u8]) -> io::Result<()> {
        create_private_dir(&self.directory)?;
        match fs::remove_file(&self.temp_file) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        {
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&self.temp_file)?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        fs::rename(&self.temp_file, &self.file)?;
        #[cfg(unix)]
        fs::File::open(&self.directory)?.sync_all()?;
        Ok(())
    }
}

/// Forgets one token of `machine_id`, only while it is still the one that was sent to (`fingerprint`), so a
/// token Apple dropped never removes the replacement the phone sent meanwhile. A phone left without tokens
/// is removed. Returns whether anything changed.
pub(crate) fn forget_live_activity_token(
    paths: &GxserverPaths,
    machine_id: &str,
    kind: TokenKind,
    fingerprint: &str,
) -> Result<bool, LiveActivityStoreError> {
    let store = LiveActivityStore::open(paths);
    let mut phones = store.read()?;
    let mut changed = false;
    for phone in phones
        .iter_mut()
        .filter(|phone| phone.machine_id == machine_id)
    {
        let slot = match kind {
            TokenKind::Activity => &mut phone.activity_token,
            TokenKind::PushToStart => &mut phone.push_to_start_token,
        };
        if slot
            .as_deref()
            .is_some_and(|token| token_fingerprint(token) == fingerprint)
        {
            *slot = None;
            changed = true;
        }
    }
    if !changed {
        return Ok(false);
    }
    phones.retain(|phone| phone.activity_token.is_some() || phone.push_to_start_token.is_some());
    store.write(&phones)?;
    Ok(true)
}

/// Every computer id appears once, every token has the relay's shape, and `seenAt` is a time.
fn phones_are_valid(phones: &[LiveActivityPhone]) -> bool {
    phones.iter().enumerate().all(|(index, phone)| {
        is_printable_id(&phone.machine_id, MACHINE_ID_MAX)
            && phone
                .activity_token
                .as_deref()
                .is_none_or(is_live_activity_token)
            && phone
                .push_to_start_token
                .as_deref()
                .is_none_or(is_live_activity_token)
            && chrono::DateTime::parse_from_rfc3339(&phone.seen_at).is_ok()
            && phone
                .device_id
                .as_deref()
                .is_none_or(|device_id| is_printable_id(device_id, DEVICE_ID_MAX))
            && phone.computers.len() <= MAX_PHONE_COMPUTERS
            && phone.computers.iter().all(|computer| {
                crate::connected_computers::is_server_id(&computer.server_id)
                    && is_printable_id(&computer.machine_id, MACHINE_ID_MAX)
            })
            && phones[..index]
                .iter()
                .all(|earlier| earlier.machine_id != phone.machine_id)
    })
}

#[cfg(unix)]
fn create_private_dir(directory: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)
}

#[cfg(not(unix))]
fn create_private_dir(directory: &Path) -> io::Result<()> {
    fs::create_dir_all(directory)
}
