//! The desktop's side of GitHub #495: the owner session with each gxserver it talks to (this
//! computer's, and every connected remote computer's through its tunnel), and approving or
//! denying approval requests with Touch ID / Windows Hello / polkit.
//!
//! - The owner key (`ghostex-owner-keys`, in the OS key store) opens a `gxo_` owner session at
//!   first use; every call to that gxserver then sends it instead of that computer's token file
//!   (`read_gpui_gxserver_auth_token` here, the remote transport for a remote computer). Until it
//!   is open, or when it can't be, the app runs on the token file's agent level.
//! - A call of the app's own that gxserver answers with `approvalRequired` (the user just clicked
//!   something sensitive) shows the system prompt at once, approves, and is sent again with
//!   `approvalId` (`typed_operations.rs`, `remote/ide_open.rs`).
//! - A request an agent made shows as a card with Approve and Deny (`ui.rs`).
//!
//! Contract: `ai/security-tokens-and-approvals.md`.
//!
//! CDXC:Security 2026-11-10 WHY: a gxserver that says `plainApprovals` (option B, `plain.rs`) needs
//! none of the owner session, owner key, approver key and signature below: this app approves there
//! with one click on its token file and opens no owner session. They stay only for computers still
//! on a gxserver from before plain approvals (11.2.2 and earlier); delete them once those no longer
//! matter. Since 2026-11-16 the owner session is opened on this computer's gxserver again for the
//! door's calls alone (pairing codes and Approve, [`door_token`]), which gxserver keeps from the
//! token file once this computer has an owner device.

pub(crate) mod handover;
pub(crate) mod plain;
pub(crate) mod re_added;
pub(crate) mod ui;

use std::{
    cell::Cell,
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use ghostex_gx_protocol::approval::{
    APPROVAL_REQUIRED_ERROR, ApprovalRequest, approval_signing_payload,
    owner_key_approval_signing_payload, owner_session_signing_payload,
};
use serde_json::{Value, json};

use crate::app::helpers::{
    gpui_gxserver_rpc_result_with_code, gpui_remote_gxserver_post_typed_operation,
};
use crate::app::model::GpuiRemoteGxserverRequestTarget;

const CALL_TIMEOUT: Duration = Duration::from_secs(15);
/// How long after a failed attempt the next call tries to open an owner session again.
const RETRY_AFTER: Duration = Duration::from_secs(60);
/// The key of this computer's own gxserver in [`SESSIONS`].
const LOCAL: &str = "local";

/// Which gxserver: this computer's, or a connected computer's through its tunnel.
#[derive(Clone)]
pub(crate) enum Transport {
    Local,
    Remote(GpuiRemoteGxserverRequestTarget),
}

impl Transport {
    /// The owner session's key: `local`, or this app's own id for that machine (its settings),
    /// never the machine's token, which the machine hands over (review round 2, R2-1).
    fn key(&self) -> String {
        match self {
            Self::Local => LOCAL.to_string(),
            Self::Remote(target) => format!("remote:{}", target.machine_id),
        }
    }
}

#[derive(Default)]
struct OwnerSession {
    token: Option<String>,
    last_attempt: Option<Instant>,
    opening: bool,
    /// The `addApproverKey` request enrolling the owner key there waits on (another owner key is
    /// enrolled on that computer): later attempts read it instead of asking again.
    waiting_approval: Option<String>,
    /// That request was denied or lapsed: no more asking there on this app's own (at connect, in
    /// the background) until the app restarts.
    gave_up: bool,
    /// `gave_up` because the request lapsed unanswered, not a Deny: the user's own click that
    /// needs it (show a pairing code, Approve) asks once more ([`ask_again_after_lapse`]).
    lapsed: bool,
    /// That gxserver takes plain approvals: no owner session is opened there (`plain.rs`).
    plain: bool,
    /// `token` was opened for door calls only ([`door_call`]): every other call keeps the token
    /// file.
    door_only: bool,
    /// When `token` was opened: a door session lapses (`DOOR_SESSION_REFRESH`).
    opened_at: Option<Instant>,
    /// The three words of this app's waiting "act as you" request there, which its card shows too
    /// (from the request's nonce and the key id, `approval::owner_key_words_input`).
    key_words: Option<String>,
    /// The server id that computer gave when this app asked there (review round 3, R3-4: the
    /// waiting message names the computer and id this app means, the card there names its own).
    server_id: Option<String>,
    /// That computer refused this app's ask because too many asked this hour (R3-2): its words.
    flooded: Option<String>,
    /// This app's own waiting "act as you" request there, which its old shared owner key may let
    /// in once the user approves it on the unlock card ([`old_key_unlock_offer`]).
    old_key_offer: Option<ApprovalRequest>,
    /// Every unlock card shown for that computer (request id → the server id it was checked
    /// for): a click is for that server id, so it still finishes after its request lapsed and
    /// was asked again ([`unlock_with_old_key`]).
    old_key_cards: HashMap<String, String>,
    /// The unlock card offered signs with a removed entry's key (CDXC:Security 2026-11-20 in
    /// re_added.rs): the name this app had for that entry ("" when it kept none), which its words
    /// say.
    old_key_removed_name: Option<String>,
}

/// A door session is opened again after this, well before gxserver lets it lapse (12 hours,
/// `DOOR_SESSION_TTL_MS` in server/src/security/policy.rs); its key is enrolled, so no card.
const DOOR_SESSION_REFRESH: Duration = Duration::from_secs(6 * 60 * 60);

static SESSIONS: Mutex<Option<HashMap<String, OwnerSession>>> = Mutex::new(None);

thread_local! {
    /// Set while this thread runs an approval or opens an owner session, so a call inside it
    /// neither prompts again nor waits on an owner session that is being opened.
    static IN_SECURITY_FLOW: Cell<bool> = const { Cell::new(false) };
    /// Set while this thread runs work the user didn't just start (a renewal, a retry loop), so
    /// an `approvalRequired` answer never shows the system prompt ([`without_prompts`]).
    static NO_PROMPTS: Cell<bool> = const { Cell::new(false) };
    /// Part of LEGACY-SHARED-KEY-SESSION: the old shared owner key's door session and the
    /// machine (its [`Transport::key`]) it is for, held only around its one Approve on this thread
    /// ([`SharedKeyDoorToken`]).
    static SHARED_KEY_DOOR_TOKEN: std::cell::RefCell<Option<(String, String)>> =
        const { std::cell::RefCell::new(None) };
    /// The server id the unlock card the user just approved was checked for, while this thread
    /// runs that click ([`unlock_with_old_key`]): the old shared owner key signs only for it.
    static OLD_KEY_CLICK: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

/// Set once the connected-computers renewal finished a full pass, so every connected machine's
/// server id is known before the old owner key signs anything ([`note_registration_pass`]).
static REGISTRATION_PASS_DONE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// The connected-computers renewal finished a full pass (`remote_conn/connected_computers.rs`).
pub(crate) fn note_registration_pass() {
    REGISTRATION_PASS_DONE.store(true, std::sync::atomic::Ordering::Release);
}

/// Part of LEGACY-SHARED-KEY-SESSION (see [`approve_with_shared_key_session`]): holds the old
/// shared key's door session in this thread's slot for one machine, and empties the slot when
/// dropped, on a panic too (review finding 6).
struct SharedKeyDoorToken;

impl SharedKeyDoorToken {
    fn hold(transport: &Transport, token: String) -> Self {
        SHARED_KEY_DOOR_TOKEN.with(|slot| *slot.borrow_mut() = Some((transport.key(), token)));
        Self
    }

    /// The held session when it is for `transport`'s machine; never another machine's.
    fn for_transport(transport: &Transport) -> Option<String> {
        let key = transport.key();
        SHARED_KEY_DOOR_TOKEN.with(|slot| {
            slot.borrow()
                .as_ref()
                .filter(|(machine, _)| *machine == key)
                .map(|(_, token)| token.clone())
        })
    }
}

impl Drop for SharedKeyDoorToken {
    fn drop(&mut self) {
        let _ = SHARED_KEY_DOOR_TOKEN.try_with(|slot| *slot.borrow_mut() = None);
    }
}

/// How soon after a failed attempt a door call tries to open the owner session again.
const DOOR_RETRY_AFTER: Duration = Duration::from_secs(5);

/*
CDXC:Security 2026-11-16 DECISION:
User (W524, "Owner only"): only this app or a paired phone makes pairing codes and approves new devices; agents running as the user, who can read the token file, must not. gxserver keeps the door for the owner session once this computer has an owner device (CDXC:Security 2026-11-16 in server/src/security/policy.rs), so this app opens its owner session on this computer's gxserver again (the owner key in the OS key store signs it; the first key on a computer with no owner key and no paired phone goes in without a card, any other waits for an Approve on a phone or the owner's app) and sends it only with those calls: the code in Connect a phone or computer, Settings > Remote and the 11.0.1 notice, and Approve ([`door_token`], picked where every call to this computer's gxserver picks its token). Every other call still goes with the token file, as since plain approvals.
*/
/*
CDXC:Security 2026-11-16 DECISION:
User (after W524: "build the cross-computer fix first"): this app gets owner power on each connected computer after one Approve per computer, so approving that computer's pairing requests from here keeps working through its connection. Where a connected computer's gxserver says its token file no longer keeps the door there (`mayApproveDevices: false` in `/api/securityStatus`, which this app reaches it with), this app enrolls its owner key there at connect: one "Let <this computer> (Ghostex app) show the pairing code of <that computer> (Ghostex server …) and approve new phones and computers there (key words: …)" card on that computer's own app or a phone paired with it, reused while it waits; denied or lapsed, it isn't asked again until this app restarts (`enroll_owner_key`). Once approved, its owner session there goes only with that computer's door calls ([`remote_door_token`]), as here. Where that computer's token file still keeps the door (it has no owner device yet) nothing is enrolled, so connecting never makes this app the owner of another computer by itself.
*/
/// A call that makes a pairing code (`openTunnel`), approves, removes a paired device or signs the
/// ask that has another computer connect back here: it goes with the owner session, which
/// gxserver asks for these once the computer has an owner device.
fn is_door_call(path: &str, params: &Value) -> bool {
    match path {
        "/api/remotePairingCode" => params.get("openTunnel").and_then(Value::as_bool) == Some(true),
        "/api/approveApprovalRequest"
        | "/api/removePairedDevice"
        | "/api/signComputerConnectBack" => true,
        _ => false,
    }
}

/// The token for a call to this computer's gxserver that makes a pairing code (`openTunnel`) or
/// approves: the owner session, opened first if needed (blocks: never on the main thread).
/// `Some(Err)` while this app waits for its own Approve on a phone, instead of sending the call
/// with the token file, which gxserver would refuse or turn into a card of its own. `None` for
/// every other call, and when the owner session can't be opened for another reason (the token
/// file still keeps the door on a computer without an owner device).
pub(crate) fn door_token(path: &str, params: &Value) -> Option<Result<String, String>> {
    door_token_for(&Transport::Local, path, params)
}

/// [`door_token`] for the connected computer `target` reaches: its owner session there, or
/// `Some(Err)` while that computer hasn't let this app act as you yet; `None` sends the call with
/// that computer's token file (it still keeps the door there, or for any other call).
pub(crate) fn remote_door_token(
    target: &GpuiRemoteGxserverRequestTarget,
    path: &str,
    params: &Value,
) -> Option<Result<String, String>> {
    door_token_for(&Transport::Remote(target.clone()), path, params)
}

fn door_token_for(
    transport: &Transport,
    path: &str,
    params: &Value,
) -> Option<Result<String, String>> {
    if !is_door_call(path, params) {
        return None;
    }
    // Part of LEGACY-SHARED-KEY-SESSION: the old key's door session, for its one Approve there.
    if let Some(token) = SharedKeyDoorToken::for_transport(transport) {
        return Some(Ok(token));
    }
    // Showing a pairing code is the user's own window (Connect a device, Settings > Remote).
    if path == "/api/remotePairingCode" {
        ask_again_after_lapse(transport);
    }
    // A pairing code, an Approve, removing a device or connecting a computer: the user's own
    // window or click, so it may ask there (once; later calls read that request).
    ensure_owner_session(transport, true);
    match with_session(&transport.key(), |session| session.token.clone()) {
        Some(token) => Some(Ok(token)),
        None if path == "/api/remotePairingCode" => owner_refusal(transport).map(Err),
        None => None,
    }
}

/// Runs `work`, an Approve of a new device, a terminal's pairing code or another owner on
/// `transport`'s gxserver, or answers why not while this app waits there for its own Approve
/// (gxserver would refuse it from the token file).
pub(crate) fn door_call<R>(
    transport: &Transport,
    work: impl FnOnce() -> Result<R, String>,
) -> Result<R, String> {
    ask_again_after_lapse(transport);
    ensure_owner_session(transport, true);
    if let Some(refusal) = owner_refusal(transport) {
        return Err(refusal);
    }
    work()
}

/*
CDXC:Security 2026-11-17 FIX:
Live, 17 Nov: m7-arch's app asked m7-arch to let it show m7-arch's own pairing code ("candle cobalt river pony"); the request lapsed unanswered after ten minutes, and the app then refused the code until it restarted, saying "Restart Ghostex to ask again". The coordinator: "If a request lapsed, the user needs an obvious way to ask again." A lapse (not a Deny) no longer ends the asking for good: the app still doesn't ask again on its own (no new card on every reconnect), but the user's own click that needs it (opening the pairing code, an Approve) asks once more, one new request that then waits a day (`OWNER_KEY_REQUEST_TTL_MS` in server/src/server/security_http.rs).
*/
/// The user's own click on `transport` after this app's ask there lapsed unanswered: ask once
/// more there (one new request; while it waits, later clicks read it instead).
fn ask_again_after_lapse(transport: &Transport) {
    with_session(&transport.key(), |session| {
        if session.gave_up && session.lapsed {
            session.gave_up = false;
            session.lapsed = false;
            session.last_attempt = None;
        }
    });
}

/// Why this app can't make a code or approve on `transport`'s gxserver yet: its owner key waits
/// for an Approve there, or that Approve was denied or lapsed. `None` when neither.
fn owner_refusal(transport: &Transport) -> Option<String> {
    let (open, waiting, gave_up, words, flooded, offered) =
        with_session(&transport.key(), |session| {
            (
                session.token.is_some(),
                session.waiting_approval.is_some(),
                session.gave_up,
                session.key_words.clone().unwrap_or_default(),
                session.flooded.clone(),
                session.old_key_offer.is_some(),
            )
        });
    let me = this_computer_name();
    let local = matches!(transport, Transport::Local);
    // The card's own question names the computer plainly (the approval cards page, 2026-11-17).
    let there = match transport {
        Transport::Local => me.clone(),
        Transport::Remote(_) => machine_and_host(transport).0,
    };
    // Only the cases that name where to approve look the owners up (one `securityStatus` call).
    let place = || owner_place(transport);
    match (open, waiting, gave_up) {
        (true, ..) => None,
        // The old owner key may let this app in there: only on the user's Approve of its card.
        (false, true, _) if offered => {
            let (computer, _) = machine_and_host(transport);
            let title = offered_unlock_title(transport);
            Some(format!(
                "First approve \"{title}\" on this Mac. Then you can show {computer}'s pairing code and approve its devices from here."
            ))
        }
        (false, false, _) if flooded.is_some() => flooded,
        (false, true, _) if local => Some(format!(
            "{} approve \"Let {me} manage {there}?\" first, only if it shows the words {words}. Then Ghostex here can show the code and approve devices.",
            capitalized(&place())
        )),
        (false, true, _) => Some(format!(
            "{} approve \"Let {me} manage {there}?\" first, only if it shows the words {words}. Then you can show {there}'s pairing code and approve its devices from here.",
            capitalized(&place())
        )),
        (false, false, true) if local => Some(format!(
            "\"Let {me} manage {there}?\" was turned down, so Ghostex here can't show the code or approve devices. Approve them {} instead, or restart Ghostex here to ask again.",
            place()
        )),
        (false, false, true) => Some(format!(
            "{there} said no to letting {me} manage it, so approve its devices {} instead. Restart Ghostex here to ask again.",
            place()
        )),
        _ => None,
    }
}

/// Where to approve this app's ask on `transport`'s gxserver: the apps and phones that own that
/// computer, named when its `securityStatus` lists them (a computer's own app by that computer's
/// name, a phone as "your phone"), otherwise "in Ghostex on the computer or phone that owns ...".
/// Never says "your phone" unless a phone is an owner there. Blocks (never on the main thread).
fn owner_place(transport: &Transport) -> String {
    let own_owner = owner_key_for(transport)
        .ok()
        .and_then(|key| key_id_of(&key.public_key_b64()));
    let status = status_for_places(transport);
    // Named by that gxserver itself where it says (CDXC:Security 2026-11-17 at `approvers_here`
    // in server/src/server/security_http.rs): owner apps on any computer, paired phones and
    // computers.
    if let Some(names) = approves_here_names(&status, transport) {
        return owner_place_text(&names, matches!(transport, Transport::Local));
    }
    let mut names: Vec<String> = Vec::new();
    for key in status
        .get("ownerKeys")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|key| own_owner.is_none() || text(key, "keyId") != own_owner)
    {
        let name = if matches!(text(key, "platform").as_deref(), Some("ios" | "android")) {
            "your phone".to_string()
        } else if let Some(label) = text(key, "label") {
            let label = label.trim_end_matches("(Ghostex app)").trim().to_string();
            if label.is_empty() {
                continue;
            }
            label
        } else {
            continue;
        };
        if !names.contains(&name) {
            names.push(name);
        }
    }
    owner_place_text(&names, matches!(transport, Transport::Local))
}

/// `transport`'s `/api/securityStatus` for naming where to approve: this app is the one asking,
/// so that computer leaves it out (`exceptKeyIds`, `exceptName`; CDXC:Security 2026-11-20 at
/// `approvers_here` in server/src/server/security_http.rs). Blocks.
fn status_for_places(transport: &Transport) -> Value {
    let params = match transport {
        Transport::Local => json!({ "exceptName": this_computer_name() }),
        Transport::Remote(target) => {
            let server_id =
                crate::app::remote_conn::connected_computers::gpui_remote_machine_server_id(
                    &target.machine_id,
                );
            // Its keys' ids only to a computer this app signed for before (review finding 5).
            if re_added::is_pinned(&target.machine_id) {
                json!({
                    "exceptKeyIds": re_added::own_key_ids(&target.machine_id, server_id.as_deref()),
                    "exceptName": this_computer_name(),
                })
            } else {
                json!({ "exceptName": this_computer_name() })
            }
        }
    };
    in_security_flow(|| rpc(transport, "/api/securityStatus", params)).unwrap_or_default()
}

/// A computer's name as its letters and digits, lower-case ("M7-DESKTOP" is "M7-Desktop", and
/// "Mohamads-MacBook-Pro-2" is "Mohamad's MacBook Pro (2)").
fn same_computer_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The names in a gxserver's `approvesHere` (`None` from one that doesn't say, or says nobody): a
/// phone as "your phone (<name>)", that computer's own app by the name this app gives it, one name
/// per computer, and never this computer, which is the one asking (its name, and the labels of
/// this app's keys there: a gxserver from before 11.12.0 lists them all, CDXC:Security 2026-11-20
/// at `approvers_here` in server/src/server/security_http.rs).
fn approves_here_names(status: &Value, transport: &Transport) -> Option<Vec<String>> {
    let mut entries: Vec<&Value> = status.get("approvesHere")?.as_array()?.iter().collect();
    // That computer's own app last: a headless computer's screen is the one nobody looks at.
    entries.sort_by_key(|entry| text(entry, "kind").as_deref() == Some("thisApp"));
    let mut asking = vec![same_computer_name(&this_computer_name())];
    if let Transport::Remote(target) = transport {
        let own = re_added::own_key_ids(&target.machine_id, text(status, "serverId").as_deref());
        asking.extend(
            status
                .get("ownerKeys")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|key| text(key, "keyId").is_some_and(|id| own.contains(&id)))
                .filter_map(|key| text(key, "label"))
                .map(|label| same_computer_name(label.trim_end_matches("(Ghostex app)"))),
        );
    }
    let there = match transport {
        Transport::Remote(_) => Some(machine_and_host(transport).0),
        Transport::Local => None,
    };
    let mut seen: Vec<String> = Vec::new();
    let names: Vec<String> = entries
        .into_iter()
        .filter_map(|entry| {
            let mut name = text(entry, "name")?;
            let same = same_computer_name(&name);
            if same.is_empty() || asking.contains(&same) || seen.contains(&same) {
                return None;
            }
            seen.push(same.clone());
            Some(match text(entry, "kind").as_deref() {
                Some("phone") => format!("your phone ({name})"),
                Some("thisApp") => {
                    // As this app calls that computer everywhere else ("M7-Desktop", not the
                    // "M7-DESKTOP" its key was labelled with).
                    if let Some(there) = there
                        .as_ref()
                        .filter(|there| same_computer_name(there) == same)
                    {
                        name = there.clone();
                    }
                    name
                }
                _ => name,
            })
        })
        .collect();
    (!names.is_empty()).then_some(names)
}

/// The places `transport`'s asks are approved, for a notice's "Approve it on …": `approvesHere`
/// where that gxserver says, else nothing (the notice then names no one).
fn approver_places(transport: &Transport) -> Vec<String> {
    approves_here_names(&status_for_places(transport), transport).unwrap_or_default()
}

/// [`owner_place`]'s words for the owners' `names` ("your phone …" is a phone).
fn owner_place_text(names: &[String], local: bool) -> String {
    if names.is_empty() {
        return if local {
            "in Ghostex on the computer or phone that owns this computer".to_string()
        } else {
            "in Ghostex on the computer or phone that owns that computer".to_string()
        };
    }
    names
        .iter()
        .map(|name| {
            if name == "your phone" {
                "on your phone".to_string()
            } else if name.starts_with("your phone (") {
                format!("on {name}")
            } else {
                format!("in Ghostex on {name}")
            }
        })
        .collect::<Vec<_>>()
        .join(" or ")
}

/// `text` with its first letter in capitals, for a place that starts a sentence.
fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The one-time value this app handed gxserver as it started it, for its first owner key there
/// (review round 2, R2-3 in server/src/security/policy.rs).
static FIRST_KEY_SECRET: Mutex<Option<String>> = Mutex::new(None);

/// Just before this app starts gxserver: a fresh one-time value in `auth/first-key-handoff`
/// (0600), which gxserver reads and deletes at start, and which this app keeps to send with its
/// first owner key. Never in any environment, so no session inherits it.
pub(crate) fn hand_first_key_secret_to_gxserver() {
    let mut bytes = [0_u8; 32];
    if ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut bytes).is_err() {
        return;
    }
    let secret = hex::encode(bytes);
    let path = first_key_handoff_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::remove_file(&path);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let written = options.open(&path).and_then(|mut file| {
        use std::io::Write as _;
        file.write_all(secret.as_bytes())
    });
    match written {
        Ok(()) => {
            *FIRST_KEY_SECRET
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(secret);
        }
        Err(error) => log::info!("first-key handoff not written: {error}"),
    }
}

/// Deletes a handoff file gxserver didn't take (review round 3, R3-1): gxserver reads and deletes it
/// before it answers, so one still there once gxserver answers belongs to a launch that failed or
/// to a gxserver that was already running. This app then forgets its value too.
pub(crate) fn forget_untaken_first_key_secret() {
    let path = first_key_handoff_path();
    if path.exists() {
        let _ = std::fs::remove_file(&path);
        *FIRST_KEY_SECRET
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}

fn first_key_handoff_path() -> std::path::PathBuf {
    crate::shared_settings::ghostex_storage_paths()
        .gxserver_state_dir()
        .join("auth")
        .join("first-key-handoff")
}

/// At start: the owner session on this computer's gxserver when it opens without anyone's
/// Approve (the first owner key, with the one-time value this app handed gxserver as it started it,
/// or this app's key already enrolled). Makes no request; a door call asks later if needed.
fn open_local_owner_session_quietly() {
    forget_untaken_first_key_secret();
    if with_session(LOCAL, |session| session.token.is_some()) {
        return;
    }
    match in_security_flow(|| open_owner_session(&Transport::Local, false)) {
        Ok(token) => with_session(LOCAL, |session| {
            if session.token.is_none() {
                session.token = Some(token);
                session.door_only = true;
                session.opened_at = Some(Instant::now());
            }
        }),
        Err(error) => log::info!("owner session for pairing not opened at start: {error}"),
    }
}

/*
CDXC:Security 2026-11-17 FIX:
User, after restarting Ghostex on the Mac once the other computers were rebuilt: "i just restarted ghostex after rebuild and saw 2 approval modals from ghostex and had to do touch id... whyyyyyyyy please fix this the right way we shouldn't spam the user with approvals like this". Each connected computer's app asked the Mac at connect, on its own, to let it show the Mac's pairing code and approve its devices (Q2099, "ask at connect"), so every restart of an app whose ask hadn't taken made a card, and the Mac showed each as a modal with Touch ID on Approve. The coordinator: no computer's app asks another computer on its own; an ask happens only as the direct result of the user's own action that needs it, and then once. Background work now only opens the owner session where this app's key is already in (`may_ask: false`: at connect, the renewal and its unlock offer, card checks, answering who may approve, after an approval); only the user's own window or click asks there (a pairing code, an Approve, removing a device, connecting a computer, Stop on a session there). Keys approved once stay in on both sides, so restarts open the session quietly and ask nothing. This supersedes Q2099's ask at connect.
*/
/// Opens the owner session on `transport`'s gxserver for door calls unless it is open or was
/// tried in the last few seconds; on a connected computer only where its token file no longer
/// keeps the door. Asks there (one request, `enroll_owner_key`) only when `may_ask`: the user's
/// own window or click needs it.
fn ensure_owner_session(transport: &Transport, may_ask: bool) {
    let key = transport.key();
    let start = with_session(&key, |session| {
        if session.door_only
            && session
                .opened_at
                .is_some_and(|opened| opened.elapsed() >= DOOR_SESSION_REFRESH)
        {
            session.token = None;
        }
        if session.token.is_some() || session.opening || session.gave_up {
            return false;
        }
        let due = session
            .last_attempt
            .is_none_or(|last| last.elapsed() >= DOOR_RETRY_AFTER);
        if due {
            session.opening = true;
            session.last_attempt = Some(Instant::now());
        }
        due
    });
    if !start {
        return;
    }
    let needed = match transport {
        Transport::Local => true,
        // A gxserver from before plain approvals keeps its own owner session (`cached_token`).
        Transport::Remote(_) => plain::server_approvals(transport)
            .is_some_and(|answer| answer.plain && !answer.may_approve_devices),
    };
    let opened = needed.then(|| in_security_flow(|| open_owner_session(transport, may_ask)));
    with_session(&key, |session| {
        session.opening = false;
        match opened {
            Some(Ok(token)) => {
                session.token = Some(token);
                session.door_only = true;
                session.opened_at = Some(Instant::now());
            }
            Some(Err(error)) => log::info!("owner session for pairing not opened: {error}"),
            None => {}
        }
    });
}

/// Whether a system prompt for one of the app's own calls is open now; a second call waits for
/// nothing and doesn't prompt (see [`approve_own_call`]).
static PROMPT_OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/*
CDXC:Security 2026-11-08 WHY:
User, on 11.1.0: "Man i see 1 million pin requests on my windows machine saying let the agents on x and x reach each other. Please don't do this! You spammed my machine with those, have like 50 open." The connected-computer renewal runs every minute on a background thread, and every `approvalRequired` answer of the app's own calls showed Windows Hello there and then, so a computer not approved yet opened a new PIN prompt each minute, each blocking its thread until answered. The system prompt is for the user's own click only: work the app does on its own runs inside `without_prompts`, waits on the one request it made, and shows it as an approval card instead (`remote_conn/connected_computers.rs`); and only one prompt is ever open at a time, whatever asks.
*/
/// Runs `work` with the system prompt off on this thread: a call answered `approvalRequired`
/// returns that answer instead of prompting.
pub(crate) fn without_prompts<R>(work: impl FnOnce() -> R) -> R {
    let outer = NO_PROMPTS.with(|flag| flag.replace(true));
    let result = work();
    NO_PROMPTS.with(|flag| flag.set(outer));
    result
}

/// The request in an answer that asks for the owner's approval: `approvalRequired` (a new
/// request) or `approvalPending` (one already waiting, server/security_gates.rs).
pub(crate) fn waiting_request_in(status: u16, body: &str) -> Option<ApprovalRequest> {
    if status != 403 {
        return None;
    }
    let envelope: Value = serde_json::from_str(body).ok()?;
    let code = envelope.get("error").and_then(Value::as_str)?;
    if code != APPROVAL_REQUIRED_ERROR && code != APPROVAL_PENDING_ERROR {
        return None;
    }
    serde_json::from_value(envelope.get("approval")?.clone()).ok()
}

/// gxserver's answer for a call that met a request already waiting (server/security_gates.rs).
pub(crate) const APPROVAL_PENDING_ERROR: &str = "approvalPending";

/// Where a request a background call waits on stands.
pub(crate) enum RequestState {
    Pending(ApprovalRequest),
    /// Approved: the call may be sent again with its id.
    Approved,
    /// Denied: don't ask again on your own.
    Declined,
    /// Lapsed unanswered (or approved and then not spent in time).
    Lapsed,
    /// Gone (gxserver restarted, or the approval was spent by another call): asking again makes
    /// at most one new request.
    Gone,
    /// gxserver didn't answer; try again later.
    Unknown,
}

/// Reads request `id` on `transport`'s gxserver, without prompting.
pub(crate) fn request_state(transport: &Transport, id: &str) -> RequestState {
    use ghostex_gx_protocol::approval::ApprovalStatus;
    let answer =
        in_security_flow(|| rpc(transport, "/api/readApprovalRequest", json!({ "id": id })));
    match answer {
        Ok(answer) => match serde_json::from_value::<ApprovalRequest>(
            answer.get("request").cloned().unwrap_or_default(),
        ) {
            Ok(request) => match request.status {
                ApprovalStatus::Pending => RequestState::Pending(request),
                ApprovalStatus::Approved => RequestState::Approved,
                // Replaced by a newer request (gxserver 11.2.3+): not a no; asking again gets
                // the newest one back.
                ApprovalStatus::Expired if request.replaced_by.is_some() => RequestState::Gone,
                ApprovalStatus::Denied => RequestState::Declined,
                ApprovalStatus::Expired => RequestState::Lapsed,
                _ => RequestState::Gone,
            },
            Err(_) => RequestState::Gone,
        },
        Err((Some(code), _)) if code == "notFound" => RequestState::Gone,
        Err(_) => RequestState::Unknown,
    }
}

fn with_session<R>(key: &str, run: impl FnOnce(&mut OwnerSession) -> R) -> R {
    let mut sessions = SESSIONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    run(sessions
        .get_or_insert_with(HashMap::new)
        .entry(key.to_string())
        .or_default())
}

fn in_security_flow<R>(run: impl FnOnce() -> R) -> R {
    let outer = IN_SECURITY_FLOW.with(|flag| flag.replace(true));
    let result = run();
    IN_SECURITY_FLOW.with(|flag| flag.set(outer));
    result
}

/// The owner session's token for `transport` when it is open. When it isn't, a background
/// thread tries to open it (at most once a minute) and this returns `None`, so the caller uses
/// the token file.
fn cached_token(transport: &Transport) -> Option<String> {
    if IN_SECURITY_FLOW.with(Cell::get) {
        return with_session(&transport.key(), |session| session.token.clone());
    }
    let key = transport.key();
    let start = with_session(&key, |session| {
        if session.token.is_some() || session.plain {
            return None;
        }
        let due = session
            .last_attempt
            .is_none_or(|last| last.elapsed() >= RETRY_AFTER);
        (due && !session.opening).then(|| {
            session.opening = true;
            session.last_attempt = Some(Instant::now());
        })
    });
    if start.is_some() {
        let transport = transport.clone();
        let _ = std::thread::Builder::new()
            .name("ghostex-owner-session".into())
            .spawn(move || {
                /*
                CDXC:Security 2026-11-10 DECISION:
                User (Q2099, "ask once per computer, earlier: ask at connect instead of when you first act there, so the Mac can answer from day one"): a connected computer that has its own owner is asked once, at connect, to let this app act as the owner there and answer its requests. The one request waits there (a day) and is read, never made again, while it waits (`enroll_owner_key`); once approved the keys are kept for good; denied or lapsed, it isn't asked again until the app restarts. This supersedes asking only when the owner first acts there (#508, CDXC:Security 2026-11-08 in server/src/server/security_http.rs, `ASK_OWNER_PARAM`).
                */
                // Option B: a gxserver that approves plainly needs no owner session.
                if plain::is_plain(&transport) {
                    with_session(&transport.key(), |session| {
                        session.opening = false;
                        session.plain = true;
                        /*
                        CDXC:Security 2026-11-17 FIX:
                        The ask at connect never ran: this thread's start stamped `last_attempt` a moment ago, so `ensure_owner_session` below took it for a try made under `DOOR_RETRY_AFTER` and did nothing, and a connected computer with its own owner was asked only once something there waited for an Approve (m7-arch had no request at all on 16 Nov). This stamp is for the token renewal, which a plain gxserver never runs; the door's own retry starts here.
                        */
                        session.last_attempt = None;
                    });
                    // The owner-only door: enroll this app's key quietly now, while gxserver may
                    // still take it without a card (CDXC:Security 2026-11-16 at `door_token`).
                    match &transport {
                        Transport::Local => open_local_owner_session_quietly(),
                        // A connected computer with its own owner: ask there once, at connect
                        // (CDXC:Security 2026-11-16 at `is_door_call`).
                        // Quietly: it opens where this app's key is already in, and asks nobody
                        // (CDXC:Security 2026-11-17 at `ensure_owner_session`).
                        Transport::Remote(_) => ensure_owner_session(&transport, false),
                    }
                    return;
                }
                let opened = in_security_flow(|| open_owner_session(&transport, false));
                with_session(&transport.key(), |session| {
                    session.opening = false;
                    match opened {
                        Ok(token) => {
                            session.token = Some(token);
                            session.door_only = false;
                        }
                        Err(error) => log::warn!("owner session not opened: {error}"),
                    }
                });
            });
    }
    // On a gxserver that approves plainly the owner session goes only with door calls
    // ([`door_call`]); everything else keeps the token file.
    with_session(&key, |session| {
        session
            .token
            .clone()
            .filter(|_| !session.plain && !session.door_only)
    })
}

/// The owner session's token for this computer's gxserver, when it is open.
pub(crate) fn cached_owner_token() -> Option<String> {
    cached_token(&Transport::Local)
}

/// The owner session's token for the remote computer `target` reaches, when it is open.
pub(crate) fn cached_remote_owner_token(
    target: &GpuiRemoteGxserverRequestTarget,
) -> Option<String> {
    cached_token(&Transport::Remote(target.clone()))
}

/// A gxserver refused owner session `token` (its state was reset): forget it, so the next call
/// uses the token file and opens a new one.
pub(crate) fn forget_owner_token(token: &str) {
    let mut sessions = SESSIONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for session in sessions.get_or_insert_with(HashMap::new).values_mut() {
        if session.token.as_deref() == Some(token) {
            session.token = None;
            session.last_attempt = None;
        }
    }
}

/// The owner session for `transport`, opened now on this thread if it isn't yet.
fn require_owner_session(transport: &Transport, may_ask: bool) -> Result<String, String> {
    if let Some(token) = with_session(&transport.key(), |session| session.token.clone()) {
        return Ok(token);
    }
    let token = open_owner_session(transport, may_ask)?;
    with_session(&transport.key(), |session| {
        session.token = Some(token.clone())
    });
    Ok(token)
}

fn rpc(
    transport: &Transport,
    endpoint: &str,
    params: Value,
) -> Result<Value, (Option<String>, String)> {
    match transport {
        Transport::Local => gpui_gxserver_rpc_result_with_code(endpoint, &params, CALL_TIMEOUT)
            .map_err(|error| (error.code, error.message)),
        Transport::Remote(target) => {
            let (status, body) =
                gpui_remote_gxserver_post_typed_operation(target, endpoint, &params, CALL_TIMEOUT)
                    .map_err(|message| (None, message))?;
            let envelope: Value = serde_json::from_str(&body).map_err(|_| {
                (
                    None,
                    "Ghostex's answer couldn't be read. Try again.".to_string(),
                )
            })?;
            if (200..300).contains(&status) {
                return Ok(envelope.get("result").cloned().unwrap_or(Value::Null));
            }
            Err((
                text(&envelope, "error"),
                text(&envelope, "message")
                    .unwrap_or_else(|| format!("gxserver request failed with HTTP {status}.")),
            ))
        }
    }
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

/// The name this computer goes by everywhere (host_name.rs), the one other computers know it by.
fn this_computer_name() -> String {
    crate::host_name::local_computer_name()
}

/// The wire form of one of this computer's keys; `device_public_key` (the owner key's) names the
/// computer, so its owner and approver keys share one device id.
fn key_json(public_key: &str, device_public_key: &str, role: &str) -> Value {
    let digest = ring::digest::digest(&ring::digest::SHA256, device_public_key.as_bytes());
    let short: String = digest.as_ref()[..6]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    /*
    CDXC:Security 2026-11-26 FIX:
    Review of d8ef5c609 (M1, L4): a test copy with its keys in a file (`GHOSTEX_OWNER_KEYS_STORE=file`) sent the same label and presence claim as a real app ("<computer> (Ghostex app)", `touchId`), so nothing told a gxserver or the person approving that any program of the user can read that key. It now says "test copy" in the label and claims no presence (`none`), and gxserver takes such a key only when it was started with the variable itself (`enroll_key` in server/src/server/security_http.rs).
    */
    let file_store = ghostex_owner_keys::uses_file_store();
    let label = if file_store {
        format!("{} (test copy, {role})", this_computer_name())
    } else {
        format!("{} ({role})", this_computer_name())
    };
    let presence = if file_store {
        ghostex_gx_protocol::approval::ApproverPresence::NoPrompt
            .as_str()
            .to_string()
    } else {
        ghostex_owner_keys::default_presence()
            .wire_name()
            .to_string()
    };
    json!({
        "keyId": "",
        "deviceId": format!("desktop-{short}"),
        "label": label,
        "platform": ghostex_owner_keys::platform_name(),
        "algorithm": "ecdsaP256Sha256",
        "publicKey": public_key,
        "presence": presence,
        "addedAtMs": 0,
    })
}

/// Enrolls the owner key on `transport`'s gxserver (trusted on first use there) and opens an
/// owner session with it.
fn open_owner_session(transport: &Transport, may_ask: bool) -> Result<String, String> {
    in_security_flow(|| {
        let key = owner_key_for(transport)?;
        let public_key = key.public_key_b64();
        remember_own_owner_key(&public_key);
        // Door sessions only: pairing codes, approvals and paired devices (review M2 in
        // server/src/security/policy.rs); a gxserver from before it ignores the field.
        let mut params = json!({
            "key": key_json(&public_key, &public_key, "Ghostex app"),
            "scope": ghostex_gx_protocol::approval::OWNER_SCOPE_DOOR,
        });
        // The approver key goes along, so the one approval that lets this app act as the owner
        // there also lets it answer requests there (server/src/server/security_http.rs,
        // `APPROVER_KEY_PARAM`); a gxserver from before it ignores the field.
        if let Ok(approver) = ghostex_owner_keys::approver_public_key_or_create() {
            params["approverKey"] = key_json(&approver, &public_key, "approvals");
        }
        if !may_ask {
            params["askOwner"] = json!(false);
        }
        // The first key on this computer goes in only with the value handed to gxserver at start.
        if matches!(transport, Transport::Local) {
            if let Some(secret) = FIRST_KEY_SECRET
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
            {
                params["firstKeySecret"] = json!(secret);
            }
        }
        let enrolled = match enroll_owner_key(transport, params.clone()) {
            // That computer's only owner may be this app's old shared key (an app before
            // 16 Nov enrolled it there): offer, or on the user's Approve of the unlock card do,
            // the one approval with it (CDXC:Security 2026-11-17 at
            // `approve_with_shared_owner_key`), then send the key again with that Approve.
            Err(waiting) if waiting == OWNER_WAITING_ERROR => {
                match approve_with_shared_owner_key(transport, &public_key, &params["key"]) {
                    Ok(true) => enroll_owner_key(transport, params)?,
                    Ok(false) => return Err(waiting),
                    // The user's own click hears why; the background only logs it.
                    Err(error) if OLD_KEY_CLICK.with(|click| click.borrow().is_some()) => {
                        return Err(error);
                    }
                    Err(error) => {
                        log::warn!("old owner key didn't let the new one in: {error}");
                        return Err(waiting);
                    }
                }
            }
            other => other?,
        };
        let key_id = enrolled
            .get("key")
            .and_then(|key| text(key, "keyId"))
            .ok_or("gxserver didn't return the owner key")?;
        let status =
            rpc(transport, "/api/securityStatus", json!({})).map_err(|(_, message)| message)?;
        let server_id = text(&status, "serverId").ok_or("gxserver didn't say its id")?;
        check_server_id(transport, &server_id, true)?;
        let challenge =
            rpc(transport, "/api/ownerChallenge", json!({})).map_err(|(_, message)| message)?;
        let challenge = text(&challenge, "challenge").ok_or("gxserver gave no challenge")?;
        let payload = owner_session_signing_payload(&server_id, &challenge, &key_id)
            .map_err(|error| error.to_string())?;
        let signature = key.sign_b64(&payload).map_err(|error| error.to_string())?;
        let opened = rpc(
            transport,
            "/api/openOwnerSession",
            json!({
                "keyId": key_id,
                "challenge": challenge,
                "signature": signature,
                "scope": ghostex_gx_protocol::approval::OWNER_SCOPE_DOOR,
                // The key's label there follows this computer's name now (CDXC:Security
                // 2026-11-20 at `current_key_label` in server/src/security/owner.rs).
                "label": format!("{} (Ghostex app)", this_computer_name()),
            }),
        )
        .map_err(|(_, message)| message)?;
        text(&opened, "token").ok_or_else(|| "gxserver opened no owner session".to_string())
    })
}

/*
CDXC:Security 2026-11-16 FIX:
Review B1 of the owner-only door: the app signed an owner session for whatever server id and challenge a computer's tunnel answered with, using the one owner key it enrolls everywhere, so whatever answered there could get a signature that opened an owner session on this computer (or another one). Each connected computer now has its own owner key (`ghostex_owner_keys::load_or_create_scoped_owner_key`, named as round 2 below says), enrolled only there, so its signature opens nothing elsewhere. On top, a connection never signs this computer's own server id, and each machine keeps the server id it first signed for (saved, round 2 below). A computer still on a gxserver from before plain approvals (11.2.2 and earlier) asks once more to let this app act as the owner there, for its new key.
*/
/*
CDXC:Security 2026-11-16 FIX:
Review round 2 (R2-1): the per-computer key, the cached owner session and the server-id pin were named after the connected computer's token, which that computer hands over, so an agent on one computer that set its token file to another computer's token got that computer's cached session, or a signature with its key. They are now named after this app's own id for the machine (its settings, `GpuiRemoteGxserverRequestTarget::machine_id`) mixed with a random salt kept in the key store (`ghostex_owner_keys::local_salt`). The pin is saved (`owner-session-pins.json` in Ghostex's state folder): each machine id keeps the server id it first signed for, and a server id already pinned to another machine is refused. Two computers that happen to share a server id (360 values) therefore can't both act as you from here; removing one and adding it again names it anew.
*/
/*
CDXC:Security 2026-11-17 FIX:
User (Q3603, "fix Ghostex first"): review B1's own key per connected computer left every computer connected before 16 Nov locked. Apps before it enrolled this app's one shared owner key there (m7-arch: its only owner key, no phone, its own app not enrolled), so gxserver there keeps the door from its token file, the new key waits for an Approve, and no device that could give it signs there any more: no phone or computer could be let in. Where that computer lists the shared key as an owner, this app uses it once, for its own waiting "act as you" ask for its new key there and nothing else.
*/
/*
CDXC:Security 2026-11-17 FIX:
User (after the first review): "add a gxserver op where the old shared key's signature approves exactly one pending request (covering server id, request id + nonce, and the new key id)", "the app uses that op when the computer's gxserver supports it", "keep a fallback for computers still on an older gxserver". A gxserver that says `approveWithOwnerKey` gets the old key's signature over that one approval (`/api/approveWithOwnerKey`), and retires the old key there once it succeeds; an older one gets the old door session (LEGACY-SHARED-KEY-SESSION below).
*/
/*
CDXC:Security 2026-11-17 FIX:
User (after the second review, whose finding 1 is a relay: an agent turns one connected computer's gxserver into a forwarder to another computer that lists the old key, and anything gxserver reports, the forwarder reports too): "Old-key signatures (new op and legacy) run only inside the user's own click (Approve / show pairing code / Settings action), never in the background, and the UI names the SSH host the app connected to. Also wait for one full connected-computers registration pass before any old-key signature." In the background this app only checks and offers: once a full registration pass is done (so every connected machine's server id is known to `check_server_id`), the renewal shows one card, "Let this Mac in on <computer>?", naming the SSH host this app reached and quoting what the computer there says it is (`old_key_unlock_offer`, `owner_security_show_unlock_card` in ui.rs). The old key signs only inside the Approve of that very card (`unlock_with_old_key`), for that one request, after Touch ID where this computer's own switch asks for it (this Mac's key, so this Mac's setting, not the remote's answer: finding 6), and only then is the server id pinned. A pairing code or Approve for that computer meanwhile says to approve that card first.
*/
/// What the old shared owner key may do for this app's waiting ask on `transport` (checked, not
/// signed): the request, the key, the server id it would sign for and whether that gxserver takes
/// the one signed approval. `None` when there is nothing to do that way, or not yet.
struct OldKeyUnlock {
    request: ApprovalRequest,
    new_key_id: String,
    shared: ghostex_owner_keys::DeviceKey,
    shared_key_id: String,
    server_id: String,
    signs_one_approval: bool,
    /// `shared` is a removed entry's key, not the old shared one (CDXC:Security 2026-11-20 in
    /// re_added.rs): the name this app had for that entry.
    removed_name: Option<String>,
}

fn check_old_key_unlock(
    transport: &Transport,
    new_public_key: &str,
) -> Result<Option<OldKeyUnlock>, String> {
    use ghostex_gx_protocol::approval::SensitiveAction;
    if matches!(transport, Transport::Local)
        || !REGISTRATION_PASS_DONE.load(std::sync::atomic::Ordering::Acquire)
    {
        return Ok(None);
    }
    let Some(request_id) =
        with_session(&transport.key(), |session| session.waiting_approval.clone())
    else {
        return Ok(None);
    };
    let RequestState::Pending(request) = request_state(transport, &request_id) else {
        return Ok(None);
    };
    let new_key_id =
        key_id_of(new_public_key).ok_or("Ghostex couldn't read this Mac's sign-in keys.")?;
    if request.action != SensitiveAction::AddApproverKey
        || request.target.as_deref() != Some(new_key_id.as_str())
    {
        return Ok(None);
    }
    let status =
        rpc(transport, "/api/securityStatus", json!({})).map_err(|(_, message)| message)?;
    let server_id = text(&status, "serverId").ok_or("gxserver didn't say its id")?;
    let signs_one_approval =
        status.get("approveWithOwnerKey").and_then(Value::as_bool) == Some(true);
    // A removed entry of this computer first (CDXC:Security 2026-11-20 in re_added.rs): its key
    // signs the one approval only, never the legacy door session.
    let Transport::Remote(target) = transport else {
        return Ok(None);
    };
    let removed = if signs_one_approval {
        re_added::removed_entry_keys(&target.machine_id, &server_id)
    } else {
        Vec::new()
    };
    let shared =
        ghostex_owner_keys::load_or_create_owner_key().map_err(|error| error.to_string())?;
    let Some((shared, removed_name)) = removed
        .into_iter()
        .map(|removed| (removed.key, Some(removed.name)))
        .chain(std::iter::once((shared, None)))
        .find(|(key, _)| re_added::lists_key(&status, key))
    else {
        return Ok(None);
    };
    let shared_key_id = key_id_of(&shared.public_key_b64())
        .ok_or("Ghostex couldn't read this Mac's sign-in keys.")?;
    check_server_id(transport, &server_id, false)?;
    if !signs_one_approval {
        // Part of LEGACY-SHARED-KEY-SESSION: the door session needs both records first.
        recorded_server_id_matches(transport, &server_id)?;
    }
    Ok(Some(OldKeyUnlock {
        request,
        new_key_id,
        shared,
        shared_key_id,
        server_id,
        signs_one_approval,
        removed_name,
    }))
}

/// Lets this app's owner key for `transport` (`new_public_key`) in with the old shared owner key,
/// only inside the user's Approve on that request's unlock card ([`unlock_with_old_key`]); any
/// other time it only keeps the offer for the card. `Ok(false)` when it didn't sign.
fn approve_with_shared_owner_key(
    transport: &Transport,
    new_public_key: &str,
    new_key: &Value,
) -> Result<bool, String> {
    let Some(unlock) = check_old_key_unlock(transport, new_public_key)? else {
        return Ok(false);
    };
    let clicked =
        OLD_KEY_CLICK.with(|click| click.borrow().as_deref() == Some(unlock.server_id.as_str()));
    if !clicked {
        with_session(&transport.key(), |session| {
            session.old_key_offer = Some(unlock.request.clone());
        });
        return Ok(false);
    }
    presence_for_unlock(transport)?;
    // The pin only now, right before the signature (first review, finding 4).
    check_server_id(transport, &unlock.server_id, true)?;
    if !unlock.signs_one_approval {
        return approve_with_shared_key_session(
            transport,
            &unlock.shared,
            &unlock.shared_key_id,
            &unlock.server_id,
            &unlock.request,
        );
    }
    let payload = owner_key_approval_signing_payload(
        &unlock.server_id,
        &unlock.request.id,
        &unlock.request.nonce,
        &unlock.new_key_id,
    )
    .map_err(|error| error.to_string())?;
    let signature = unlock
        .shared
        .sign_b64(&payload)
        .map_err(|error| error.to_string())?;
    rpc(
        transport,
        "/api/approveWithOwnerKey",
        json!({
            "id": unlock.request.id,
            "keyId": unlock.shared_key_id,
            "signature": signature,
            // gxserver enrolls it in the same step, so a lost answer leaves nothing half done.
            "key": new_key,
        }),
    )
    .map_err(|(_, message)| message)?;
    Ok(true)
}

/// Touch ID, Windows Hello or the password before the old owner key signs, when this computer's
/// own Settings > Security switch asks for it (it is this computer's key; second review, finding
/// 6): one prompt at a time (`PROMPT_OPEN`), naming the computer and the SSH host.
fn presence_for_unlock(transport: &Transport) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    if !plain::this_computer_requires_presence() {
        return Ok(());
    }
    if PROMPT_OPEN
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("Another approval prompt is open. Finish it, then approve again.".to_string());
    }
    // macOS shows it as "Ghostex is trying to <reason>." (CDXC:Security 2026-11-17 at
    // `machine_and_host`).
    let title = offered_unlock_title(transport);
    let reason = title.trim_end_matches('?').replacen("Let", "let", 1);
    let confirmed = ghostex_owner_keys::confirm_presence(&reason);
    PROMPT_OPEN.store(false, Ordering::Release);
    confirmed.map(|_| ()).map_err(|error| error.to_string())
}

/*
CDXC:Security 2026-11-17 FIX:
User, on the Touch ID prompt "Ghostex is trying to let this Mac in on m7-arch (Easy Connect at tcpGFwWC…) with its old owner key": it showed the whole Easy Connect address and jargon. "Rewrite the prompt reason (and the card text if it has the same problem) for non-technical users: short, no addresses, ids, keys or codes. E.g. 'let this Mac manage m7-arch again'. For SSH, at most the plain host name, never the long Easy Connect address. Keep the security-relevant bit (which computer) visible in plain words." The prompt and the card name the computer by the name saved in Settings > Remote; the card adds the name that computer reports for itself (what a relay to another computer would change) and, for SSH, the host name only when it is a name rather than a numeric address.
*/
/// The name this app keeps for `transport`'s machine (Settings > Remote), and its SSH host name
/// when that is a plain name (never an address, never an Easy Connect code).
fn machine_and_host(transport: &Transport) -> (String, Option<String>) {
    let Transport::Remote(target) = transport else {
        return (this_computer_name(), None);
    };
    let computer = crate::app::helpers::gpui_remote_machine_name_from_settings(&target.machine_id)
        .unwrap_or_else(|| "that computer".to_string());
    let snapshot = crate::shared_settings::shared_sidebar_settings_snapshot();
    let saved = snapshot
        .object()
        .get("remoteMachines")
        .and_then(Value::as_array)
        .and_then(|machines| {
            machines.iter().find(|machine| {
                crate::app::helpers::gpui_remote_machine_id_from_value(machine).as_deref()
                    == Some(target.machine_id.as_str())
            })
        })
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let easy_connect = saved.get("transport").and_then(Value::as_str) == Some("easyConnect");
    let host = crate::app::helpers::gpui_remote_machine_string_field(&saved, "sshHost")
        .filter(|_| !easy_connect)
        .filter(|host| {
            host.len() <= 40
                && host.parse::<std::net::IpAddr>().is_err()
                && host.chars().any(|c| c.is_ascii_alphabetic())
                && !host.eq_ignore_ascii_case(&computer)
        });
    (computer, host)
}

/// The name the computer behind `request` gives itself: the "<name> (Ghostex server …)" gxserver
/// writes into its "act as you" summary (`this_computer_label` in
/// server/src/server/security_http.rs).
pub(crate) fn reported_computer_name(request: &ApprovalRequest) -> Option<String> {
    let after = request.summary.split_once("pairing code of ")?.1;
    let name = after.split_once(" (Ghostex server")?.0.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/*
CDXC:Security 2026-11-17 FIX:
Live check on m7-arch (the user approved "the m7-arch unlock" on the Mac; m7-arch got nothing after 02:54): this app's "act as you" request there, made at connect, waits ten minutes (`PENDING_TTL_MS`). Once it lapsed unanswered, the next try read it as declined, asked once without making a request, was refused, and gave up on that computer until this app restarted; the unlock card was checked only against the lapsed request, so it never came back, and a card left on screen from before the lapse failed when clicked. The user: "fix it so after any restart of either side the app asks again and the card appears." Where that computer still lists this app's old shared key and its token file no longer keeps the door, each registration pass now asks again (one request, reused while it waits) whenever no request of this app waits there (it lapsed, the other side lost it, or this app restarted), then offers the card for it; a lapse there no longer gives up (`enroll_owner_key`), only a Deny does. A click is for the server id its card was checked for, not its request id, so a card clicked after its request lapsed asks again and lets the new request in with the same click.
*/
/// The unlock card's offer for `target`'s machine, checked in the background after a full
/// registration pass (`remote_conn/connected_computers.rs`): this app's waiting "act as you"
/// request there, asked again first when none waits, when its old shared owner key may let it
/// in. Never signs.
pub(crate) fn old_key_unlock_offer(
    target: &GpuiRemoteGxserverRequestTarget,
) -> Option<ApprovalRequest> {
    let transport = Transport::Remote(target.clone());
    let key = transport.key();
    if with_session(&key, |session| session.token.is_some() || session.gave_up)
        || !REGISTRATION_PASS_DONE.load(std::sync::atomic::Ordering::Acquire)
    {
        return None;
    }
    // Only where that computer's door is its owner's and this app's old key is one of them.
    let locked = plain::server_approvals(&transport)
        .is_some_and(|answer| answer.plain && !answer.may_approve_devices);
    if !locked || !in_security_flow(|| lists_old_shared_key(&transport)) {
        return None;
    }
    let waiting = with_session(&key, |session| session.waiting_approval.clone());
    let still_waiting = waiting.is_some_and(|id| {
        matches!(
            in_security_flow(|| request_state(&transport, &id)),
            RequestState::Pending(_)
        )
    });
    if !still_waiting {
        // Nothing of this app's waits there: the key may be in already (its session opens
        // quietly), else no card until the user's own click there asks (opening that computer's
        // pairing code or approving its device; CDXC:Security 2026-11-17 at `ensure_owner_session`).
        if let Ok(token) = in_security_flow(|| open_owner_session(&transport, false)) {
            with_session(&key, |session| {
                session.token = Some(token);
                session.door_only = true;
                session.opened_at = Some(Instant::now());
            });
        }
        return None;
    }
    let new_public_key = owner_key_for(&transport).ok()?.public_key_b64();
    let checked = in_security_flow(|| check_old_key_unlock(&transport, &new_public_key));
    match checked {
        Ok(Some(unlock)) => {
            with_session(&key, |session| {
                session.old_key_offer = Some(unlock.request.clone());
                session.old_key_removed_name = unlock.removed_name.clone();
                session
                    .old_key_cards
                    .insert(unlock.request.id.clone(), unlock.server_id.clone());
            });
            Some(unlock.request)
        }
        Ok(None) => None,
        Err(error) => {
            log::info!("old owner key not offered: {error}");
            None
        }
    }
}

/// Whether `transport`'s gxserver lists one of this app's old owner keys as an owner (not
/// retired): the old shared one, or a removed entry's for that computer (CDXC:Security 2026-11-20
/// in re_added.rs).
fn lists_old_shared_key(transport: &Transport) -> bool {
    let Transport::Remote(target) = transport else {
        return false;
    };
    let Ok(status) = rpc(transport, "/api/securityStatus", json!({})) else {
        return false;
    };
    let removed = text(&status, "serverId")
        .map(|server_id| re_added::removed_entry_keys(&target.machine_id, &server_id))
        .unwrap_or_default();
    removed
        .iter()
        .map(|removed| &removed.key)
        .chain(ghostex_owner_keys::load_or_create_owner_key().ok().as_ref())
        .any(|key| re_added::lists_key(&status, key))
}

/// The unlock card's title and words for `request` on `target`'s machine, in plain words: what
/// Approve does, and which computer answered (the name it reports for itself, and the SSH host
/// name when it is a plain name), so a card from the wrong computer reads wrong.
pub(crate) fn old_key_unlock_card(
    target: &GpuiRemoteGxserverRequestTarget,
    request: &ApprovalRequest,
) -> (String, String) {
    let (computer, host) = machine_and_host(&Transport::Remote(target.clone()));
    let reached = host
        .map(|host| format!(" This Mac connects to it at {host}."))
        .unwrap_or_default();
    // The name the computer that answered gives itself shows as the card's own row, "It says it
    // is" (notice_modal_host.rs `notice_item`), with "Approve only if that is the computer you
    // expect." under it; this is the plain body (the approval cards page, 2026-11-17).
    let _ = request;
    let removed = with_session(&Transport::Remote(target.clone()).key(), |session| {
        session.old_key_removed_name.clone()
    });
    // A removed entry's key: what this app itself recorded, never what that computer says, so a
    // computer answering as another reads wrong here (CDXC:Security 2026-11-20 in re_added.rs).
    let why = match removed.as_deref().map(str::trim) {
        Some("") => format!(
            "This Mac managed a computer with the same server id before, under an entry you removed from Settings > Remote. Approve only if {computer} is that computer."
        ),
        Some(name) => format!(
            "This Mac managed it before as \"{name}\", an entry you removed from Settings > Remote. Approve only if {computer} is that computer."
        ),
        None => format!(
            "After the last update, {computer} no longer knows this Mac, so nobody can add phones or computers to it."
        ),
    };
    (
        unlock_card_title(&computer, removed.is_some()),
        format!("{why}{reached}"),
    )
}

/// The unlock card's question: "again" only for the old shared key, which that computer listed as
/// this app's; a removed entry's key may be for another entry's computer (re_added.rs).
fn unlock_card_title(computer: &str, removed_entry: bool) -> String {
    if removed_entry {
        format!("Let this Mac manage {computer}?")
    } else {
        format!("Let this Mac manage {computer} again?")
    }
}

/// The unlock card's question for `transport`'s machine as last offered.
fn offered_unlock_title(transport: &Transport) -> String {
    let (computer, _) = machine_and_host(transport);
    let removed = with_session(&transport.key(), |session| {
        session.old_key_removed_name.is_some()
    });
    unlock_card_title(&computer, removed)
}

/// Approve on the unlock card for `request`, the user's own click: the old shared owner key
/// signs for that request alone, then this app's key there opens its door session.
pub(crate) fn unlock_with_old_key(
    transport: &Transport,
    request: &ApprovalRequest,
) -> Result<(), String> {
    // The server id this card was checked for; a card this app never offered signs nothing.
    let Some(server_id) = with_session(&transport.key(), |session| {
        session.old_key_cards.get(&request.id).cloned()
    }) else {
        return Err("This card is out of date. Wait for a new one, then approve that.".to_string());
    };
    // A lapse is no "no": the click may ask again there (CDXC:Security 2026-11-17 at
    // `old_key_unlock_offer`).
    with_session(&transport.key(), |session| session.gave_up = false);
    let outer = OLD_KEY_CLICK.with(|click| click.replace(Some(server_id)));
    let opened = in_security_flow(|| open_owner_session(transport, true));
    OLD_KEY_CLICK.with(|click| *click.borrow_mut() = outer);
    let token = opened.map_err(|error| {
        if error == OWNER_WAITING_ERROR {
            let (computer, _) = machine_and_host(transport);
            format!(
                "This Mac couldn't manage {computer} just now. Try again in a minute. If it keeps failing, approve this Mac in Ghostex on the computer or phone that owns {computer}."
            )
        } else {
            error
        }
    })?;
    with_session(&transport.key(), |session| {
        session.token = Some(token);
        session.door_only = true;
        session.opened_at = Some(Instant::now());
        session.old_key_offer = None;
        session.old_key_cards.clear();
    });
    Ok(())
}

/*
LEGACY-SHARED-KEY-SESSION (CDXC:Security 2026-11-17): the fallback for a connected computer whose gxserver doesn't say `approveWithOwnerKey` yet (before 17 Nov). Delete it once every connected computer runs a newer gxserver; the user will ask. Delete together: this function, `recorded_server_id_matches`, `SharedKeyDoorToken` with `SHARED_KEY_DOOR_TOKEN` and its check in `door_token_for`, and the two `!signs_one_approval` branches in `check_old_key_unlock` and `approve_with_shared_owner_key` (a computer without the op then gets no unlock card).
It opens a door session with the old shared key (door routes only; 12 hours on gxserver, this app drops it after the one Approve) and approves the request with it. On a gxserver from before 17 Nov the scope isn't signed and the old key isn't retired, so such a signature, relayed, could open a session on any computer listing that key: only the user's click on a card naming the SSH host, the server-id checks and the records below stand in the way.
*/
/// Part of LEGACY-SHARED-KEY-SESSION (above): approves `request` with a door session of the old
/// shared key `shared`, held only for this one call to `transport`'s machine.
fn approve_with_shared_key_session(
    transport: &Transport,
    shared: &ghostex_owner_keys::DeviceKey,
    shared_key_id: &str,
    server_id: &str,
    request: &ApprovalRequest,
) -> Result<bool, String> {
    let challenge =
        rpc(transport, "/api/ownerChallenge", json!({})).map_err(|(_, message)| message)?;
    let challenge = text(&challenge, "challenge").ok_or("gxserver gave no challenge")?;
    let payload = owner_session_signing_payload(server_id, &challenge, shared_key_id)
        .map_err(|error| error.to_string())?;
    let signature = shared
        .sign_b64(&payload)
        .map_err(|error| error.to_string())?;
    let opened = rpc(
        transport,
        "/api/openOwnerSession",
        json!({
            "keyId": shared_key_id,
            "challenge": challenge,
            "signature": signature,
            "scope": ghostex_gx_protocol::approval::OWNER_SCOPE_DOOR,
        }),
    )
    .map_err(|(_, message)| message)?;
    let token = text(&opened, "token").ok_or("gxserver opened no owner session")?;
    let held = SharedKeyDoorToken::hold(transport, token);
    let approved = rpc(
        transport,
        "/api/approveApprovalRequest",
        json!({ "id": request.id, "understands": [ghostex_gx_protocol::approval::UNDERSTANDS_SERVER_LINK] }),
    );
    drop(held);
    approved.map_err(|(_, message)| message)?;
    Ok(true)
}

/*
CDXC:Security 2026-11-17 FIX:
User (1A, "match against what the local gxserver recorded for that computer; refuse to sign with the old key when there is no recorded match"): part of LEGACY-SHARED-KEY-SESSION. Before the old key signs a door session, the server id it would sign for must be the one this app registered for that machine (`connected_computers`, from its `/api/health/server` at each renewal) and one this computer's gxserver lists among its connected computers (`/api/listConnectedComputers`, any entry with that id); with no record or a different one, nothing is offered or signed, and the next registration pass checks again. Both records come from that machine's own answers on its connection, so this makes the answers agree with each other, not with an independent record: none was kept before 16 Nov. The local list alone could be written by an agent here, which is why this app's own record must match too.
*/
/// Part of LEGACY-SHARED-KEY-SESSION: refuses to sign a door session with the old shared owner
/// key for `server_id` unless this app's registration of `transport`'s machine and this
/// computer's gxserver both record that server id.
fn recorded_server_id_matches(transport: &Transport, server_id: &str) -> Result<(), String> {
    const REFUSED: &str = "Ghostex couldn't confirm this is the same computer it connected to before, so it changed nothing there.";
    let Transport::Remote(target) = transport else {
        return Err(REFUSED.to_string());
    };
    let registered = crate::app::remote_conn::connected_computers::gpui_remote_machine_server_id(
        &target.machine_id,
    );
    if registered.as_deref() != Some(server_id) {
        log::warn!("old owner key not used: the app's record of that machine's server id differs");
        return Err(REFUSED.to_string());
    }
    let listed =
        gpui_gxserver_rpc_result_with_code("/api/listConnectedComputers", &json!({}), CALL_TIMEOUT)
            .map_err(|error| error.message)?;
    let recorded_here = listed
        .get("computers")
        .and_then(Value::as_array)
        .is_some_and(|computers| {
            computers
                .iter()
                .any(|computer| text(computer, "serverId").as_deref() == Some(server_id))
        });
    if !recorded_here {
        log::warn!("old owner key not used: this computer's gxserver records no such computer");
        return Err(REFUSED.to_string());
    }
    Ok(())
}

/// The owner key this app signs with on `transport`: this computer's, or the one of that
/// connected computer.
fn owner_key_for(transport: &Transport) -> Result<ghostex_owner_keys::DeviceKey, String> {
    match transport {
        Transport::Local => ghostex_owner_keys::load_or_create_owner_key(),
        Transport::Remote(target) => {
            if target.machine_id.is_empty() {
                return Err(
                    "This connection names no machine, so this app doesn't sign in there as you."
                        .to_string(),
                );
            }
            ghostex_owner_keys::load_or_create_scoped_owner_key(&re_added::scoped_key_name(
                &target.machine_id,
            )?)
        }
    }
    .map_err(|error| error.to_string())
}

/// The saved pins: this app's machine id → the server id it signed for there.
fn pins_path() -> std::path::PathBuf {
    crate::shared_settings::ghostex_storage_paths()
        .state_dir
        .join("owner-session-pins.json")
}

static PINS_LOCK: Mutex<()> = Mutex::new(());

/// Refuses to sign an owner session for `server_id` on `transport` when it is this computer's own
/// id over a connection to another computer, not the id that machine signed for before, or the id
/// another machine signed for. `pin`: keep `server_id` as that machine's from now on; only right
/// before a signature, so an attempt refused or cancelled before it pins nothing (review finding 4).
fn check_server_id(transport: &Transport, server_id: &str, pin: bool) -> Result<(), String> {
    const REFUSED: &str =
        "That computer answered as a different computer, so Ghostex changed nothing there.";
    let Transport::Remote(target) = transport else {
        return Ok(());
    };
    if crate::app::helpers::gpui_local_gxserver_server_id().as_deref() == Some(server_id) {
        log::warn!("owner session not signed: a connection named this computer's server id");
        return Err(REFUSED.to_string());
    }
    let _guard = PINS_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let path = pins_path();
    let mut pins: std::collections::BTreeMap<String, String> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    // A removed entry's pin doesn't block an entry that first registered with its server id
    // (CDXC:Security 2026-11-20 in re_added.rs).
    let configured = re_added::configured_machine_ids();
    if !re_added::pins_allow_for(&pins, &target.machine_id, server_id, configured.as_ref()) {
        log::warn!(
            "owner session not signed: that server id is another machine's, or not this one's"
        );
        return Err(REFUSED.to_string());
    }
    if pins.contains_key(&target.machine_id) {
        // Pinned before: another machine reporting the same id later doesn't lock it out
        // (second review, finding 4).
        return Ok(());
    }
    /*
    CDXC:Security 2026-11-17 FIX:
    Found while proving `/api/approveWithOwnerKey`: a connection that forwards everything to another computer this app also reaches (an agent replaced that machine's gxserver with a relay) looks like that computer, so the old key's one signed approval would let this app's key for the relaying machine in there, and its door sessions, opened through the relay, would pass through it. The pins stop that only once this app signed for the real computer first. So a server id that another connected machine reported at its own registration (`connected_computers`) is refused too: the real computer's own connection names it, which a relay elsewhere can't change. Its limit: that machine must have registered in this run (connected, at its one-minute renewal); the old owner key waits for a full registration pass for that reason. Second review, finding 4 (server ids have only 360 values, so two computers can share one): it applies only to a machine not pinned yet, so a computer pinned before keeps working when another machine reports its id, and a newly reported duplicate (refused here and by the pins) can't block it.
    */
    if crate::app::remote_conn::connected_computers::gpui_other_remote_machine_has_server_id(
        &target.machine_id,
        server_id,
    ) {
        log::warn!("owner session not signed: another connected machine has that server id");
        return Err(REFUSED.to_string());
    }

    if !pin {
        return Ok(());
    }
    pins.retain(|machine, pinned| {
        pinned != server_id
            || !re_added::takes_over(&target.machine_id, server_id, machine, configured.as_ref())
    });
    pins.insert(target.machine_id.clone(), server_id.to_string());
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let text = serde_json::to_string_pretty(&pins).map_err(|error| error.to_string())?;
    std::fs::write(&path, text)
        .map_err(|error| format!("Ghostex couldn't save which computer this is: {error}"))?;
    Ok(())
}

/*
CDXC:Security 2026-11-08 WHY:
When another owner key is already enrolled on a computer (a second computer connecting to it), enrolling this app's owner key there asks for that owner's approval. The owner session is retried every minute, and each request lapsed after ten minutes, so the Mac's log gained a new "Let M7-DESKTOP (Ghostex app) (windows) act as the owner of Ghostex on this computer" request every ten minutes all day (30 on 8 Nov). The app now asks once: it waits on that request, sends its key again with the approval once it is approved, and stops asking there until it restarts when it is denied or lapses.
*/
/// The key ids gxserver gives this app's owner keys (hex of the first 16 bytes of SHA-256 over
/// the public key; one per computer), learned the first time the app sends each anywhere.
static OWN_OWNER_KEY_IDS: Mutex<Option<std::collections::HashSet<String>>> = Mutex::new(None);

fn remember_own_owner_key(public_key_b64: &str) {
    if let Some(key_id) = key_id_of(public_key_b64) {
        OWN_OWNER_KEY_IDS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get_or_insert_with(Default::default)
            .insert(key_id);
    }
}

/// gxserver's key id for a public key: hex of the first 16 bytes of SHA-256 over its bytes.
fn key_id_of(public_key_b64: &str) -> Option<String> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(public_key_b64)
        .ok()?;
    let digest = ring::digest::digest(&ring::digest::SHA256, &bytes);
    Some(hex::encode(&digest.as_ref()[..16]))
}

/// Whether `request` came back from one of this app's own calls (its own click, approved where
/// it happens, or its background work, which shows its own card): never by a key's device id,
/// which two Macs share after Migration Assistant (CDXC:Security 2026-11-10 in `plain.rs`).
pub(crate) fn asked_by_this_app(request: &ApprovalRequest) -> bool {
    plain::is_own_request(&request.id)
}

/// Whether `request` asks a connected computer to let this very app act as the owner there: only
/// that computer's own owner can answer it, so this app never shows it as a card.
pub(crate) fn asks_for_this_app(request: &ApprovalRequest) -> bool {
    request.action == ghostex_gx_protocol::approval::SensitiveAction::AddApproverKey
        && request.target.is_some()
        && request.target.as_deref().is_some_and(|target| {
            OWN_OWNER_KEY_IDS
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .as_ref()
                .is_some_and(|ids| ids.contains(target))
        })
}

/*
CDXC:Security 2026-11-09 WHY:
Discord report (11.2.0): "I click approve, and then get this little error popup that tells me its Not approved. It tells me to do it on the phone or other computers. Nothing comes up on those computers and I dont see a way to do it there?" The request a computer waits on showed as a card only from its `approvalRequestsChanged` event, sent once when the request is made: an app that was restarting, updating or reconnecting at that moment never showed it, and nothing listed it later, so the request waited a day with no way to answer it. Each computer's waiting requests are now read when its stream goes live and shown as cards, where this app may answer them (this computer, or one where it already acts as the owner).
*/
/// The requests waiting for the owner on `transport`'s gxserver. `None` when they can't be
/// listed: a connected computer where this app doesn't act as the owner yet (listing needs that,
/// and it is never asked for from here), or gxserver didn't answer.
pub(crate) fn pending_requests(transport: &Transport) -> Option<Vec<ApprovalRequest>> {
    use ghostex_gx_protocol::approval::ApprovalStatus;
    in_security_flow(|| {
        match transport {
            // A gxserver that approves plainly lists them for this app's token file.
            _ if plain::is_plain(transport) => {}
            Transport::Local => {
                require_owner_session(transport, false)?;
            }
            // Opened here when that needs no approval: the stream usually goes live before the
            // background opener ran, which listed nothing and left requests made meanwhile with
            // no card.
            Transport::Remote(_) => {
                if quiet_owner_session(transport).is_none() {
                    return Err("this app doesn't act as the owner there yet".to_string());
                }
            }
        }
        let answer = rpc(transport, "/api/listApprovalRequests", json!({}))
            .map_err(|(_, message)| message)?;
        Ok::<_, String>(
            answer
                .get("requests")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|request| {
                    serde_json::from_value::<ApprovalRequest>(request.clone()).ok()
                })
                .filter(|request| request.status == ApprovalStatus::Pending)
                .collect(),
        )
    })
    .map_err(|error| log::info!("waiting approval requests not read: {error}"))
    .ok()
}

/// gxserver's routes for the user's own actions on a session (Stop, answering its prompt, typing
/// into it): the owner's devices do them without asking, anyone else asks each time.
const STEER_ENDPOINTS: &[&str] = &[
    "/api/interruptSessionChat",
    "/api/answerSessionChatPrompt",
    "/api/sendSessionText",
    "/api/sendSessionEnter",
    "/api/sendSessionMessage",
];

/// The code of the answer [`steer_needs_owner`] gives instead of sending the call.
pub(crate) const OWNER_APPROVAL_NEEDED_ERROR: &str = "ownerApprovalNeeded";

/*
CDXC:Security 2026-11-08 WHY:
User, after Escape in the chat of a session on their connected Windows PC showed a red "Waiting for your approval … Stop what session G506f is doing … repeat this call with approvalId": "wtf is this line". The owner's own Stop, answers and typing on a connected computer's sessions pass without asking once this app holds an owner session there; this Mac had none, because Windows has its own owner and the request to let this Mac act as the owner there was never approved, so each action went as that computer's token file (agent level) and asked on its own. Now such an action first makes sure of the owner session; while it waits on that one approval, the action is not sent (no request per action) and the chat says once, in plain words, what to approve and where.
*/
/// For a Stop, prompt answer or typing on the computer `target` reaches: `None` when it may go
/// (an owner session there is open, or can be opened now), or the plain refusal to show when this
/// app is not yet approved as the owner there.
pub(crate) fn steer_needs_owner(
    target: &GpuiRemoteGxserverRequestTarget,
    endpoint: &str,
) -> Option<String> {
    if !STEER_ENDPOINTS.contains(&endpoint) || IN_SECURITY_FLOW.with(Cell::get) {
        return None;
    }
    let transport = Transport::Remote(target.clone());
    let key = transport.key();
    if with_session(&key, |session| session.token.is_some() || session.plain)
        || plain::known_approvals(&transport).is_some_and(|answer| answer.plain)
    {
        return None;
    }
    // Never block the window: the background opener (`cached_token`) tries meanwhile.
    if std::thread::current().name() != Some("main") {
        if let Ok(token) = require_owner_session(&transport, true) {
            with_session(&key, |session| session.token = Some(token));
            return None;
        }
    }
    let waiting = with_session(&key, |session| {
        session.waiting_approval.is_some() || session.gave_up
    });
    waiting.then(|| {
        let me = this_computer_name();
        let computer =
            crate::app::helpers::gpui_remote_machine_name_from_settings(&target.machine_id)
                .unwrap_or_else(|| "that computer".to_string());
        format!(
            "{me} can't stop, answer or type in {computer}'s sessions yet. Approve \"Let {me} manage {computer}?\" once in Ghostex on a computer or phone that owns {computer}; after that it works without asking."
        )
    })
}

/// [`enroll_owner_key`]'s answer while that computer's own owner hasn't let this app act as the
/// owner there; the approval card replaces it with words naming both computers.
pub(crate) const OWNER_WAITING_ERROR: &str = "This computer can't manage that computer yet. Approve the card asking to let it manage that computer in Ghostex on a computer or phone that owns it, then try again.";

/// What Approve on `computer`'s card says while that computer's owner hasn't let this app in.
pub(crate) fn owner_waiting_message(computer: &str) -> String {
    let me = this_computer_name();
    format!(
        "Ghostex here can't answer {computer}'s asks yet. Approve \"Let {me} manage {computer}?\" in Ghostex on a computer or phone that owns {computer}, or answer this ask there instead."
    )
}

/// `/api/enrollOwnerKey` on `transport`, waiting on the approval it asked for there instead of
/// asking again.
fn enroll_owner_key(transport: &Transport, params: Value) -> Result<Value, String> {
    const WAITING: &str = OWNER_WAITING_ERROR;
    const STOPPED: &str = "This app wasn't approved as an owner on that computer; it uses that computer's token file until Ghostex restarts.";
    let key = transport.key();
    let (waiting, gave_up) = with_session(&key, |session| {
        (session.waiting_approval.clone(), session.gave_up)
    });
    if gave_up {
        return Err(STOPPED.to_string());
    }
    let mut params = params;
    if let Some(id) = waiting {
        match request_state(transport, &id) {
            RequestState::Pending(request) => {
                note_key_words(transport, &request);
                return Err(WAITING.to_string());
            }
            RequestState::Unknown => return Err(WAITING.to_string()),
            RequestState::Approved => {
                params = with_approval_id(&params, &id).unwrap_or(params);
                // Spending the owner's approval asks nobody: a gxserver from before 17 Nov
                // refuses `askOwner: false` before it reads the id.
                if let Some(fields) = params.as_object_mut() {
                    fields.remove("askOwner");
                }
            }
            // Approved and then lapsed unspent reads the same as denied, yet the key went in when
            // it was approved (CDXC:Security 2026-11-10 in server/src/security/approvals.rs): ask
            // once more without making a request, and stop only if it isn't there.
            state @ (RequestState::Declined | RequestState::Lapsed) => {
                with_session(&key, |session| session.waiting_approval = None);
                let mut quiet = params.clone();
                quiet["askOwner"] = json!(false);
                let (status, body) = raw_rpc(transport, "/api/enrollOwnerKey", &quiet)?;
                let envelope: Value = serde_json::from_str(&body).unwrap_or_default();
                if (200..300).contains(&status)
                    && envelope.get("ok").and_then(Value::as_bool) == Some(true)
                {
                    return Ok(envelope.get("result").cloned().unwrap_or(Value::Null));
                }
                // Lapsed where this app's old key could still let it in: ask again below, so
                // its unlock card comes back (CDXC:Security 2026-11-17 at `old_key_unlock_offer`).
                let ask_again = matches!(state, RequestState::Lapsed)
                    && matches!(transport, Transport::Remote(_))
                    && lists_old_shared_key(transport);
                if !ask_again {
                    with_session(&key, |session| {
                        session.gave_up = true;
                        session.lapsed = matches!(state, RequestState::Lapsed);
                    });
                    return Err(STOPPED.to_string());
                }
            }
            RequestState::Gone => {}
        }
        with_session(&key, |session| session.waiting_approval = None);
    }
    let (status, body) = raw_rpc(transport, "/api/enrollOwnerKey", &params)?;
    let refused: Value = serde_json::from_str(&body).unwrap_or_default();
    // Too many asked to act as you there this hour (review round 3, R3-2): say so, ask later.
    if text(&refused, "error").as_deref() == Some("ownerKeyRequestsFlooded") {
        let message = text(&refused, "message").unwrap_or_else(|| {
            "Too many apps asked to manage that computer this hour. Try again later.".to_string()
        });
        with_session(&key, |session| session.flooded = Some(message.clone()));
        return Err(message);
    }
    if text(&refused, "error").as_deref() == Some(OWNER_APPROVAL_NEEDED_ERROR) {
        return Err(
            "This app isn't approved as an owner on that computer yet; it asks there once."
                .to_string(),
        );
    }
    if let Some(request) = waiting_request_in(status, &body) {
        note_key_words(transport, &request);
        plain::note_own_request(&request.id);
        with_session(&key, |session| {
            session.waiting_approval = Some(request.id.clone())
        });
        return Err(WAITING.to_string());
    }
    let envelope: Value = serde_json::from_str(&body)
        .map_err(|_| "Ghostex's answer couldn't be read. Try again.".to_string())?;
    if (200..300).contains(&status) && envelope.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok(envelope.get("result").cloned().unwrap_or(Value::Null));
    }
    Err(text(&envelope, "message")
        .unwrap_or_else(|| format!("gxserver request failed with HTTP {status}.")))
}

/// The computer this app means on `transport`, as its messages name it: its saved name and the
/// server id it gave when this app asked there (review round 3, R3-4).
fn expected_computer(transport: &Transport) -> String {
    match transport {
        Transport::Local => this_computer_name(),
        Transport::Remote(target) => {
            let name =
                crate::app::helpers::gpui_remote_machine_name_from_settings(&target.machine_id)
                    .unwrap_or_else(|| "That computer".to_string());
            match with_session(&transport.key(), |session| session.server_id.clone()) {
                Some(server_id) => format!("{name} (Ghostex server {server_id})"),
                None => name,
            }
        }
    }
}

/// Keeps the words of this app's own "act as you" request on `transport`, worked out here
/// from the request's nonce and the key id it names (review round 2, R2-2 in
/// server/src/server/security_http.rs), for the waiting messages.
fn note_key_words(transport: &Transport, request: &ApprovalRequest) {
    let Some(key_id) = request.target.as_deref() else {
        return;
    };
    let digest = ring::digest::digest(
        &ring::digest::SHA256,
        &ghostex_gx_protocol::approval::owner_key_words_input(&request.nonce, key_id),
    );
    let words = ghostex_gx_protocol::approval::owner_key_words(digest.as_ref());
    let server_id = rpc(transport, "/api/securityStatus", json!({}))
        .ok()
        .and_then(|status| text(&status, "serverId"));
    with_session(&transport.key(), |session| {
        session.key_words = Some(words);
        session.flooded = None;
        if server_id.is_some() {
            session.server_id = server_id;
        }
    });
}

/// One call on `transport`, answered as HTTP status and body (the app's own transport: it sends
/// the owner session when one is open).
fn raw_rpc(transport: &Transport, endpoint: &str, params: &Value) -> Result<(u16, String), String> {
    match transport {
        Transport::Local => {
            crate::app::helpers::gxserver_post_typed_operation(endpoint, params, CALL_TIMEOUT)
        }
        Transport::Remote(target) => {
            gpui_remote_gxserver_post_typed_operation(target, endpoint, params, CALL_TIMEOUT)
        }
    }
}

/// The owner session on `transport` when it is open or opens without asking anyone (this app's key
/// is enrolled there, or it is the first); `None` otherwise. Never makes a request.
fn quiet_owner_session(transport: &Transport) -> Option<String> {
    let key = transport.key();
    if let Some(token) = with_session(&key, |session| session.token.clone()) {
        return Some(token);
    }
    if with_session(&key, |session| session.gave_up) {
        return None;
    }
    let token = in_security_flow(|| open_owner_session(transport, false)).ok()?;
    with_session(&key, |session| session.token = Some(token.clone()));
    Some(token)
}

/*
CDXC:Security 2026-11-17 FIX:
Live, 17 Nov: the user approved m7-arch's app's "show the pairing code of <the Mac>" at 03:39 and it lapsed two minutes later unused. Told of the approval, this app dropped the request's id and only noted that the next call should open the session; no call came within the approval's two minutes (the ask at connect had run, and nothing else asks while no window needs a code), and a later one would have asked anew without the id. Now the approval itself brings this app back at once, with the request's id kept: a gxserver from before 17 Nov lets the key in only on that call within two minutes, a newer one has let it in already (`enroll_owner_key_on_approval` in server/src/server/security_http.rs) and the call just opens the session. This computer's own requests count too (m7-arch's app's ask on m7-arch).
*/
/// That computer let this app act as the owner there (the request asking it was approved): the
/// owner session opens now, coming back with the approval's id, on a background thread.
pub(crate) fn note_owner_key_approved(transport: &Transport) {
    with_session(&transport.key(), |session| {
        session.gave_up = false;
        session.lapsed = false;
        session.last_attempt = None;
    });
    let transport = transport.clone();
    let _ = std::thread::Builder::new()
        .name("ghostex-owner-key-approved".into())
        .spawn(move || ensure_owner_session(&transport, false));
}

/// The user's own connect to a computer whose terminal let this one in while only its own app
/// owned it: ask once to manage it (CDXC:Security 2026-11-17 DECISION at `terminal_may_approve` in
/// server/src/security/policy.rs). The words to match in that terminal while it waits, `None`
/// when this app manages it already.
pub(crate) fn ask_to_manage(
    target: &GpuiRemoteGxserverRequestTarget,
    handover: Option<&str>,
) -> Result<Option<String>, String> {
    let transport = Transport::Remote(target.clone());
    let key = transport.key();
    with_session(&key, |session| {
        session.gave_up = false;
        session.lapsed = false;
    });
    match in_security_flow(|| open_owner_session(&transport, true)) {
        Ok(token) => {
            with_session(&key, |session| {
                session.token = Some(token);
                session.door_only = true;
                session.opened_at = Some(Instant::now());
            });
            Ok(None)
        }
        Err(error) if error == OWNER_WAITING_ERROR => {
            // That computer approves this ask from its terminal only once this app proves it
            // through its SSH login there, with the one-time secret it gave this computer
            // (CDXC:Security 2026-11-17 DECISION at `terminal_may_approve`).
            if let Some(secret) = handover.map(str::trim).filter(|secret| !secret.is_empty()) {
                let asked = with_session(&key, |session| session.waiting_approval.clone())
                    .ok_or_else(|| "The ask to manage that computer went away.".to_string())?;
                handover::prove_handover(&target.machine_id, &asked, secret)?;
            }
            Ok(with_session(&key, |session| session.key_words.clone()))
        }
        Err(error) => Err(error),
    }
}

/// Who may answer a computer's requests.
pub(crate) enum Answerer {
    /// This app: Approve here signs it.
    ThisApp,
    /// Only the named devices there (Ghostex on another computer, a paired phone; empty when
    /// gxserver doesn't say). `asked`: a request waits there to let this app answer from here.
    Elsewhere {
        devices: Vec<String>,
        asked: bool,
        /// One more sentence for the notice: the key words and computer the card there must show
        /// (R3-4), or why this app couldn't ask (too many asks this hour, R3-2).
        note: Option<String>,
        /// Nothing of this app's waits there yet: the notice offers "Let <this computer> manage
        /// <that computer>" ([`ask_to_manage`]; CDXC:Security 2026-11-20 in ui.rs).
        can_ask: bool,
    },
}

/*
CDXC:Security 2026-11-10 WHY:
A Ghostex user with four computers: "Clicking approve on that gave me the error that the other machines is where that should happen." This app showed every connected computer's requests as cards with Approve, but it can only sign for a computer where it acts as the owner and its approver key is enrolled, which a computer with its own Ghostex app grants only once its owner approves it. So a connected computer's request is an approvable card only where this app can answer it; elsewhere it is a notice that names where to approve it, and this app asks that computer once (one card there) to let it answer its requests from here too.
*/
/// [`answerer`] for one request: on a gxserver that approves plainly, a new device, a terminal's
/// pairing code or another owner needs this app's owner session there once that computer has an
/// owner device (CDXC:Security 2026-11-16 at `is_door_call`); until that computer lets this app
/// act as you, it is a notice naming where to approve it.
pub(crate) fn answerer_for(transport: &Transport, request: &ApprovalRequest) -> Answerer {
    let plain_door = matches!(transport, Transport::Remote(_))
        && plain::is_plain(transport)
        && plain::opens_door(&request.action);
    if !plain_door {
        return answerer(transport);
    }
    ensure_owner_session(transport, false);
    let open = with_session(&transport.key(), |session| session.token.is_some());
    if open || plain::server_approvals(transport).is_none_or(|answer| answer.may_approve_devices) {
        return Answerer::ThisApp;
    }
    let (asked, words, flooded, offered) = with_session(&transport.key(), |session| {
        (
            session.waiting_approval.is_some(),
            session.key_words.clone(),
            session.flooded.clone(),
            session.old_key_offer.is_some(),
        )
    });
    let there = expected_computer(transport);
    if offered {
        let title = offered_unlock_title(transport);
        return Answerer::Elsewhere {
            devices: Vec::new(),
            asked,
            note: Some(format!(
                "Or first approve \"{title}\" on this Mac, then approve this here."
            )),
            can_ask: false,
        };
    }
    Answerer::Elsewhere {
        devices: approver_places(transport),
        asked,
        can_ask: !asked && flooded.is_none(),
        note: flooded.or_else(|| {
            words.filter(|_| asked).map(|words| {
                format!(
                    "Approve that ask only if it shows the key words {words} and names {there}."
                )
            })
        }),
    }
}

/// Whether this app can answer `transport`'s requests now, and if not, who can. Runs on a
/// background thread; may ask that computer once to let this app act as the owner there.
pub(crate) fn answerer(transport: &Transport) -> Answerer {
    if matches!(transport, Transport::Local) || plain::is_plain(transport) {
        return Answerer::ThisApp;
    }
    in_security_flow(|| {
        let own_approver = ghostex_owner_keys::approver_public_key_or_create()
            .ok()
            .and_then(|public_key| key_id_of(&public_key));
        let mut session = quiet_owner_session(transport);
        if session.is_none() && !with_session(&transport.key(), |session| session.gave_up) {
            // Quietly: asking there is the user's own click (CDXC:Security 2026-11-17 at
            // `ensure_owner_session`).
            if let Ok(token) = open_owner_session(transport, false) {
                with_session(&transport.key(), |state| state.token = Some(token.clone()));
                session = Some(token);
            }
        }
        let asked = with_session(&transport.key(), |session| {
            session.waiting_approval.is_some()
        });
        let status = rpc(transport, "/api/securityStatus", json!({})).unwrap_or_default();
        let labels = |field: &str, suffix: &str| -> Vec<String> {
            status
                .get(field)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|key| own_approver.is_none() || text(key, "keyId") != own_approver)
                .filter_map(|key| {
                    // A phone's key is labelled with whatever the phone sent ("Phone" on 11.2.1).
                    if matches!(text(key, "platform").as_deref(), Some("ios" | "android")) {
                        return Some("your phone".to_string());
                    }
                    text(key, "label")
                        .map(|label| label.trim_end_matches(suffix).trim().to_string())
                })
                .fold(Vec::new(), |mut names: Vec<String>, name| {
                    if !names.contains(&name) {
                        names.push(name);
                    }
                    names
                })
        };
        let approvers = labels("approverKeys", "(approvals)");
        let approver_ids: Vec<String> = status
            .get("approverKeys")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|key| text(key, "keyId"))
            .collect();
        let elsewhere = || {
            if approvers.is_empty() {
                labels("ownerKeys", "(Ghostex app)")
            } else {
                approvers.clone()
            }
        };
        if session.is_none() {
            return Answerer::Elsewhere {
                devices: elsewhere(),
                asked,
                note: None,
                can_ask: false,
            };
        }
        if approver_ids.is_empty()
            || own_approver
                .as_ref()
                .is_some_and(|own| approver_ids.contains(own))
        {
            return Answerer::ThisApp;
        }
        // The owner there let this app in before its approver key came along with it: ask for
        // that key once (a card there, `addApproverKey`), and send people there meanwhile.
        if let (Ok(owner_key), Ok(approver)) = (
            owner_key_for(transport),
            ghostex_owner_keys::approver_public_key_or_create(),
        ) {
            let wire_key = key_json(&approver, &owner_key.public_key_b64(), "approvals");
            match rpc(
                transport,
                "/api/enrollApproverKey",
                json!({ "key": wire_key }),
            ) {
                Ok(_) => return Answerer::ThisApp,
                Err((code, _)) => {
                    return Answerer::Elsewhere {
                        devices: elsewhere(),
                        asked: code.as_deref() == Some(APPROVAL_REQUIRED_ERROR),
                        note: None,
                        can_ask: false,
                    };
                }
            }
        }
        Answerer::Elsewhere {
            devices: elsewhere(),
            asked: false,
            note: None,
            can_ask: false,
        }
    })
}

/// Shows the system prompt with the request's summary, signs the request with this computer's
/// approver key (enrolling it on that gxserver the first time) and approves it there.
pub(crate) fn approve_request(
    transport: &Transport,
    request: &ApprovalRequest,
) -> Result<(), String> {
    if let Some(answer) = plain::server_approvals(transport).filter(|answer| answer.plain) {
        return plain::approve_plain(transport, request, answer.require_presence);
    }
    in_security_flow(|| {
        require_owner_session(transport, true)?;
        let owner_key = owner_key_for(transport)?;
        let (key, presence) =
            ghostex_owner_keys::unlock_approver_key(&plain::prompt_reason(transport, request))
                .map_err(|error| error.to_string())?;
        let mut wire_key = key_json(
            &key.public_key_b64(),
            &owner_key.public_key_b64(),
            "approvals",
        );
        wire_key["presence"] = json!(presence.wire_name());
        let enrolled = rpc(
            transport,
            "/api/enrollApproverKey",
            json!({ "key": wire_key }),
        )
        .map_err(|(code, message)| match code.as_deref() {
            Some(APPROVAL_REQUIRED_ERROR) => {
                "Approve adding this computer as an approver in Ghostex on the computer or phone that owns that computer first, then try again."
                    .to_string()
            }
            _ => message,
        })?;
        let key_id = enrolled
            .get("key")
            .and_then(|key| text(key, "keyId"))
            .ok_or("gxserver didn't return the approver key")?;
        let status =
            rpc(transport, "/api/securityStatus", json!({})).map_err(|(_, message)| message)?;
        let server_id = text(&status, "serverId").ok_or("gxserver didn't say its id")?;
        let payload =
            approval_signing_payload(&server_id, request).map_err(|error| error.to_string())?;
        let signature = key.sign_b64(&payload).map_err(|error| error.to_string())?;
        rpc(
            transport,
            "/api/approveApprovalRequest",
            json!({ "id": request.id, "keyId": key_id, "signature": signature, "understands": [ghostex_gx_protocol::approval::UNDERSTANDS_SERVER_LINK] }),
        )
        .map(|_| ())
        .map_err(|(_, message)| message)
    })
}

/*
CDXC:Security 2026-11-08 WHY:
A card only leaves when its gxserver says the request was answered or lapsed, and a card whose word never arrived (the app was away from its event stream when it happened) stayed for good: Approve asked for Touch ID and then failed, and the agent's next try added another card beside it. Before the prompt, the card's request is read back; one no longer waiting just closes.
*/
/*
CDXC:Security 2026-11-10 WHY:
Gregg's report (gregg-approval-bug): after a gxserver restart, or once a newer request replaced the card's, Approve read the old id back as gone and said "That request was already answered or ran out of time", while the fresh request sat with no card. The user: a card never dead-ends on a request that is gone. gxserver now keeps waiting requests across restarts and points a replaced id at the newest one (`current`); the card moves there (its words may differ, so it is shown to be read, not approved for the user), and one that truly lapsed says so plainly ("This request expired. Ask again from the phone."), never "already answered".
*/
/// Where an approval card's request stands, read just before Approve or Deny.
pub(crate) enum CardRequest {
    /// Still waiting (or gxserver can't say: the decision goes ahead and reports its own error).
    Waiting,
    /// A newer request for the same thing replaced it: its card shows instead.
    Moved(ApprovalRequest),
    /// No longer waiting: the plain words why.
    Gone(String),
}

/// Reads request `id` back before the card's Approve or Deny.
pub(crate) fn card_request(transport: &Transport, id: &str) -> CardRequest {
    use ghostex_gx_protocol::approval::{ApprovalStatus, GONE_MESSAGE, expired_message};
    let plain = plain::is_plain(transport);
    in_security_flow(|| {
        if !plain {
            require_owner_session(transport, true)?;
        }
        let answer = match rpc(transport, "/api/readApprovalRequest", json!({ "id": id })) {
            Ok(answer) => answer,
            Err((Some(code), _)) if code == "notFound" => {
                return Ok::<_, String>(CardRequest::Gone(GONE_MESSAGE.to_string()));
            }
            Err(_) => return Ok(CardRequest::Waiting),
        };
        let read = |key: &str| {
            answer
                .get(key)
                .cloned()
                .and_then(|value| serde_json::from_value::<ApprovalRequest>(value).ok())
        };
        let Some(request) = read("request") else {
            return Ok(CardRequest::Waiting);
        };
        Ok(match request.status {
            ApprovalStatus::Pending => CardRequest::Waiting,
            ApprovalStatus::Expired => match read("current") {
                Some(current) if current.status == ApprovalStatus::Pending => {
                    CardRequest::Moved(current)
                }
                _ if request.replaced_by.is_some() => CardRequest::Gone(
                    "A newer request for the same thing replaced this one, and it isn't waiting any more."
                        .to_string(),
                ),
                _ => CardRequest::Gone(expired_message(&request)),
            },
            ApprovalStatus::Denied => CardRequest::Gone("This request was denied.".to_string()),
            _ => CardRequest::Gone("This request was already approved.".to_string()),
        })
    })
    .unwrap_or(CardRequest::Waiting)
}

/// [`approve_request`] for request `id` of this computer's gxserver (a phone or computer waiting
/// to pair, from Settings > Remote or the 11.0.1 notice): reads it, then prompts and signs.
pub(crate) fn approve_local_request_id(id: &str) -> Result<(), String> {
    let transport = Transport::Local;
    let plain = plain::is_plain(&transport);
    let request = in_security_flow(|| {
        if !plain {
            require_owner_session(&transport, true)?;
        }
        let answer = rpc(&transport, "/api/readApprovalRequest", json!({ "id": id }))
            .map_err(|(_, message)| message)?;
        serde_json::from_value::<ApprovalRequest>(
            answer.get("request").cloned().unwrap_or_default(),
        )
        .map_err(|_| "This request is no longer waiting.".to_string())
    })?;
    approve_request(&transport, &request)
}

/// Denies a request; no prompt, saying no only closes a door.
pub(crate) fn deny_request(transport: &Transport, id: &str) -> Result<(), String> {
    let plain = plain::is_plain(transport);
    in_security_flow(|| {
        if !plain {
            require_owner_session(transport, true)?;
        }
        rpc(transport, "/api/denyApprovalRequest", json!({ "id": id }))
            .map(|_| ())
            .map_err(|(_, message)| message)
    })
}

/*
CDXC:Security 2026-11-08 WHY:
A sensitive change the user starts in the app (turning on Easy Connect, a security-relaxing setting; letting a connected computer in waits as a card instead, CDXC:Security 2026-11-08 at `without_prompts`) reaches gxserver as an owner call that still needs a signed approval. Showing the system prompt right then, with gxserver's summary, and sending the call again is what the user expects from their own click; a card would make them approve their own click twice. Never on the main thread (the prompt would freeze the window) and never inside an approval or owner-session flow already running.
*/
/// For a call of the app's own that `transport`'s gxserver answered with `approvalRequired`
/// (`body`): prompts, approves, and returns the approval id to send the call again with. `None`
/// when the answer isn't one, the user didn't approve, or this thread can't prompt.
pub(crate) fn approve_own_call(transport: &Transport, status: u16, body: &str) -> Option<String> {
    if status != 403 || IN_SECURITY_FLOW.with(Cell::get) || NO_PROMPTS.with(Cell::get) {
        return None;
    }
    if std::thread::current().name() == Some("main") {
        return None;
    }
    let envelope: Value = serde_json::from_str(body).ok()?;
    if envelope.get("error").and_then(Value::as_str) != Some(APPROVAL_REQUIRED_ERROR) {
        return None;
    }
    let request: ApprovalRequest =
        serde_json::from_value(envelope.get("approval")?.clone()).ok()?;
    plain::note_own_request(&request.id);
    // One system prompt at a time, whatever asks: a second call meanwhile returns its answer.
    if PROMPT_OPEN
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
        )
        .is_err()
    {
        log::info!("own call not prompted: another approval prompt is open");
        return None;
    }
    let approved = approve_request(transport, &request);
    PROMPT_OPEN.store(false, std::sync::atomic::Ordering::Release);
    match approved {
        Ok(()) => Some(request.id),
        Err(error) => {
            log::info!("own call not approved: {error}");
            None
        }
    }
}

/// `params` with `approvalId` set, for the call sent again after its approval.
pub(crate) fn with_approval_id(params: &Value, approval_id: &str) -> Option<Value> {
    let mut params = params.clone();
    params
        .as_object_mut()?
        .insert("approvalId".to_string(), json!(approval_id));
    Some(params)
}
