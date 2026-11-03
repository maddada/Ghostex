use std::{
    collections::BTreeMap,
    fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, PoisonError},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::paths::GxserverPaths;

use super::limiter::PhoneBudget;
use super::registration::{is_expo_push_token, is_printable_id, DEVICE_ID_MAX, MACHINE_ID_MAX};

const STORE_FILE_NAME: &str = "push-registrations.json";
const STORE_TEMP_FILE_NAME: &str = "push-registrations.json.tmp";
const STORE_VERSION: u32 = 1;
/// Expo ticket ids are UUIDs; anything longer is not one.
pub(crate) const RECEIPT_ID_MAX: usize = 128;

/// The phone's three category switches; a kind that is off is never pushed to that phone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PushKinds {
    pub(crate) finished: bool,
    pub(crate) waiting: bool,
    pub(crate) error: bool,
}

/// One phone's registration on this computer, keyed by the id the phone uses for this computer.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PushRegistration {
    pub(crate) token: String,
    pub(crate) machine_id: String,
    pub(crate) kinds: PushKinds,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) device_id: Option<String>,
    pub(crate) registered_at: String,
}

// The token is a send credential, so a debug print of a registration never shows it.
impl fmt::Debug for PushRegistration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PushRegistration")
            .field("token", &"<redacted>")
            .field("machine_id", &self.machine_id)
            .field("kinds", &self.kinds)
            .field("device_id", &self.device_id)
            .field("registered_at", &self.registered_at)
            .finish()
    }
}

/// An Expo ticket whose receipt is still to be read, and the registration it was sent to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PendingReceipt {
    pub(crate) id: String,
    pub(crate) machine_id: String,
    pub(crate) token_hash: String,
    pub(crate) due_at: i64,
    pub(crate) expires_at: i64,
}

/// Everything the private file holds: the registrations, each phone's send budget, and the
/// receipts still to read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PushStoreData {
    pub(crate) registrations: Vec<PushRegistration>,
    pub(crate) budgets: BTreeMap<String, PhoneBudget>,
    pub(crate) receipts: Vec<PendingReceipt>,
}

impl PushStoreData {
    pub(crate) fn registration(&self, machine_id: &str) -> Option<&PushRegistration> {
        self.registrations
            .iter()
            .find(|registration| registration.machine_id == machine_id)
    }

    /// Deletes `machine_id`'s registration only while it still holds the token that was sent to,
    /// so a dead-token report never removes the replacement a rotation registered.
    pub(crate) fn remove_if_token(&mut self, machine_id: &str, token_hash: &str) -> bool {
        let before = self.registrations.len();
        self.registrations.retain(|registration| {
            registration.machine_id != machine_id
                || token_fingerprint(&registration.token) != token_hash
        });
        self.registrations.len() != before
    }
}

/// The SHA-256 of a token: lets queued work and receipts name the token they were for without
/// keeping a second copy of it.
pub(crate) fn token_fingerprint(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoreFile {
    version: u32,
    registrations: Vec<PushRegistration>,
    #[serde(default)]
    budgets: BTreeMap<String, PhoneBudget>,
    #[serde(default)]
    receipts: Vec<PendingReceipt>,
}

/// `StoreFile` borrowed from the data being written.
#[derive(Serialize)]
struct StoreFileOut<'a> {
    version: u32,
    registrations: &'a [PushRegistration],
    #[serde(skip_serializing_if = "no_budgets")]
    budgets: &'a BTreeMap<String, PhoneBudget>,
    #[serde(skip_serializing_if = "no_receipts")]
    receipts: &'a [PendingReceipt],
}

fn no_budgets(budgets: &&BTreeMap<String, PhoneBudget>) -> bool {
    budgets.is_empty()
}

fn no_receipts(receipts: &&[PendingReceipt]) -> bool {
    receipts.is_empty()
}

#[derive(Debug)]
pub(crate) enum PushStoreError {
    Unreadable(io::Error),
    Damaged,
    WriteFailed(io::Error),
}

impl fmt::Display for PushStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable(error) => write!(
                formatter,
                "The phone push registrations in gxserver's auth folder could not be read ({error})."
            ),
            Self::Damaged => write!(
                formatter,
                "The phone push registrations file {STORE_FILE_NAME} in gxserver's auth folder is damaged. Delete it to reset every phone's push registration; the phones register again."
            ),
            Self::WriteFailed(error) => write!(
                formatter,
                "The phone push registrations could not be saved ({error}); the previous registrations are unchanged."
            ),
        }
    }
}

impl std::error::Error for PushStoreError {}

static STORE_LOCK: Mutex<()> = Mutex::new(());

/// CDXC:Notifications 2026-10-31 WHY:
/// Phone push tokens are send credentials, so they live in one private file next to gxserver's own auth token (folder 0700, file 0600 on macOS and Linux), keyed by the computer id each phone sends; the settings file, the state database and every endpoint stay free of them.
/// The same file keeps each phone's recent send times and the Expo receipts still to read, so a gxserver restart neither resets the rate limits nor forgets to check whether a token died. Receipts and queued work name a token by its SHA-256, never by a copy.
/// Unlike the work-mode credentials file, a damaged or unreadable file is reported instead of read as empty: reading it as empty would let the next registration overwrite every other phone's entry and would let an unpair report success while a token linked to that device stays behind.
/// Holding a store handle holds the one process-wide lock, so registration, unregistration, unpair cleanup and the sender read and replace the file one at a time; the sender never holds it during a network request.
/// A write replaces the file only after the new bytes and the rename are flushed to disk; a failed write removes its temporary file and leaves the previous file in place.
pub(crate) struct PushRegistrationStore {
    directory: PathBuf,
    file: PathBuf,
    temp_file: PathBuf,
    _guard: MutexGuard<'static, ()>,
}

impl PushRegistrationStore {
    pub(crate) fn open(paths: &GxserverPaths) -> Self {
        // The lock guards no data, so a panic in another holder leaves nothing inconsistent.
        let guard = STORE_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        Self {
            directory: paths.auth_dir.clone(),
            file: paths.auth_dir.join(STORE_FILE_NAME),
            temp_file: paths.auth_dir.join(STORE_TEMP_FILE_NAME),
            _guard: guard,
        }
    }

    pub(crate) fn read(&self) -> Result<PushStoreData, PushStoreError> {
        let bytes = match fs::read(&self.file) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(PushStoreData::default())
            }
            Err(error) => return Err(PushStoreError::Unreadable(error)),
        };
        let store: StoreFile =
            serde_json::from_slice(&bytes).map_err(|_| PushStoreError::Damaged)?;
        if store.version != STORE_VERSION
            || !registrations_are_valid(&store.registrations)
            || !receipts_are_valid(&store.receipts)
            || !store.budgets.iter().all(|(machine_id, budget)| {
                is_printable_id(machine_id, MACHINE_ID_MAX) && budget.is_valid()
            })
        {
            return Err(PushStoreError::Damaged);
        }
        Ok(PushStoreData {
            registrations: store.registrations,
            budgets: store.budgets,
            receipts: store.receipts,
        })
    }

    pub(crate) fn write(&self, data: &PushStoreData) -> Result<(), PushStoreError> {
        let bytes = serde_json::to_vec(&StoreFileOut {
            version: STORE_VERSION,
            registrations: &data.registrations,
            budgets: &data.budgets,
            receipts: &data.receipts,
        })
        .map_err(|error| PushStoreError::WriteFailed(error.into()))?;
        let result = self.replace_file(&bytes);
        if result.is_err() {
            let _ = fs::remove_file(&self.temp_file);
        }
        result.map_err(PushStoreError::WriteFailed)
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
        sync_directory(&self.directory)
    }
}

/// Removes every registration linked to a paired device. Returns how many were removed.
pub(crate) fn remove_device_push_registrations(
    store: &PushRegistrationStore,
    device_id: &str,
) -> Result<usize, PushStoreError> {
    let mut data = store.read()?;
    let before = data.registrations.len();
    data.registrations
        .retain(|registration| registration.device_id.as_deref() != Some(device_id));
    let removed = before - data.registrations.len();
    if removed > 0 {
        store.write(&data)?;
    }
    Ok(removed)
}

/// Each computer id, token and device id appears at most once, and every field has the shape
/// registration accepts.
fn registrations_are_valid(registrations: &[PushRegistration]) -> bool {
    registrations
        .iter()
        .enumerate()
        .all(|(index, registration)| {
            is_expo_push_token(&registration.token)
                && is_printable_id(&registration.machine_id, MACHINE_ID_MAX)
                && registration
                    .device_id
                    .as_deref()
                    .is_none_or(|device_id| is_printable_id(device_id, DEVICE_ID_MAX))
                && chrono::DateTime::parse_from_rfc3339(&registration.registered_at).is_ok()
                && registrations[..index].iter().all(|earlier| {
                    earlier.machine_id != registration.machine_id
                        && earlier.token != registration.token
                        && (earlier.device_id.is_none()
                            || earlier.device_id != registration.device_id)
                })
        })
}

fn receipts_are_valid(receipts: &[PendingReceipt]) -> bool {
    receipts.iter().all(|receipt| {
        is_printable_id(&receipt.id, RECEIPT_ID_MAX)
            && is_printable_id(&receipt.machine_id, MACHINE_ID_MAX)
            && receipt.token_hash.len() == 64
            && receipt
                .token_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
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

#[cfg(unix)]
fn sync_directory(directory: &Path) -> io::Result<()> {
    fs::File::open(directory)?.sync_all()
}

// Windows offers no directory handle to flush; the rename itself is the commit there.
#[cfg(not(unix))]
fn sync_directory(_directory: &Path) -> io::Result<()> {
    Ok(())
}
