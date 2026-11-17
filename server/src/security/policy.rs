//! Which sensitive actions skip the owner's signed approval, and which approvals are remembered
//! once given: the one rule both gates read (`security_gates::gate` for whole routes, and
//! [`SecurityState::require_owner_presence`] for the handlers that ask themselves).

use ghostex_gx_protocol::approval::{ApprovalRequest, SensitiveAction};

use super::{ActionRequest, Principal, SecurityState};

/*
CDXC:Security 2026-11-16 DECISION:
User (W524, "Owner only"): only the Ghostex app on this computer or a paired phone makes pairing codes and approves new devices; agents running as the user must not. The token file is readable by every program of the user (its path is in every session's environment), so once this computer has an owner device (the app's owner key, kept in the OS key store, is enrolled, or a phone paired with its own token), the token file alone no longer:
- gets a pairing code without an Approve on the app or a phone (`ghostex computer-code` asks once, then gets codes for ten minutes, `TOKEN_FILE_CODE_WINDOW_MS`; since 2026-11-17 Q3723 it gets the code at once, CDXC:Security 2026-11-17 DECISION at `pairing_code_gate` in remote_access/http.rs);
- approves a device asking to pair, a terminal's pairing code or another owner key (`/api/approveApprovalRequest` refuses it; it may still deny, which only closes a door);
- adds an owner key without such an Approve (`addApproverKey`).
The app proves itself with its owner session (`/api/openOwnerSession`, signed by the owner key), which it opens for these calls only; every other call it makes, and everything else the token file does, is unchanged. Until this computer has any owner device (a new install, or a server without the app and no phone yet), the token file still keeps the door, or nobody could pair the first phone. A program running as the user that rewrites gxserver's own files can still change all this (the same-user limit). This narrows the 2026-11-10 decision below for the door only.
*/
/*
CDXC:Security 2026-11-16 FIX:
Review B2 of the owner-only door: the two minutes after the first start with this rule let the token file add an owner key even where the app's key was already enrolled or a phone paired, took any key, stayed open after the first one, and opened again when an unreadable `security.json` was moved aside. An upgrade is exactly when gxserver restarts with agent sessions still running. The user kept the window for its intent (the app that started gxserver enrolls without a card), so it now takes a key only while this computer has no owner key and no paired phone, and closes with the first key: the same rule as the first key on a new computer, so it needs no timer of its own. A computer upgraded with a phone but without the app's key asks the phone once. A paired phone in the database marks the computer as owned at every start (`note_owner_device`, server/foreground.rs), so a `security.json` moved aside never reopens it where a phone is paired.
*/
/// Remembered once this computer has an owner device (a paired phone; an owner key counts by
/// itself): from then on the token file alone no longer keeps the door.
pub const OWNER_DEVICE: &str = "ownerDevice";
/*
CDXC:Security 2026-11-16 FIX:
Review round 2 (R2-3): on a computer without the app, the first owner key went in from the token file with no card, so an agent could make itself the only keeper of the door and `ghostex computer-code` would then wait for an app or phone that doesn't exist. The first key now needs a one-time value the app hands gxserver as it starts it: the app writes it to `auth/first-key-handoff` (0600) just before it launches gxserver, gxserver reads and deletes that file first thing at start (server/foreground.rs), keeps only its hash in memory, never puts it in any environment, and spends it on the first key (`take_first_key`). On a computer where the app never starts gxserver, no key can be added without an Approve, so the token file keeps the door until a phone pairs and `ghostex computer-code` works there as before. Where gxserver was started by something else (the CLI, a restart outside the app), the app stays on the token file, which still keeps the door there, until the app next starts gxserver. Known same-user limits (review round 3, R3-1): whoever starts gxserver picks the value. gxserver takes whatever file is there when it starts, so any program running as the user can write that file itself and start gxserver (the gxserver binary, `launchctl kickstart`, a detached process), then add the first key with that value; on a computer without the app and without a phone that makes it the only one who can approve, and the user is locked out until the reset in Help. It could also read the file in the moment between the app writing it and gxserver deleting it. The app deletes a file gxserver didn't take (a failed launch, or a gxserver that was already running), so none is left lying around. Where the token file already kept the door, this adds no power beyond that lockout.
*/
/*
CDXC:Security 2026-11-16 FIX:
Review round 3 (R3-2): the cap of three waiting "act as you" requests could be filled by any token-file holder, so the app's own request was refused here or on a connected computer. Now a request that carries this start's one-time value (this computer's app) is never capped or pushed out; any other new request pushes out the oldest waiting one that isn't the app's (as replaced, so its asker may ask again); and the token file makes at most `OWNER_KEY_REQUESTS_PER_HOUR` new ones an hour. Past that it is refused with plain words, which the asking app shows, and the security log notes it once an hour.
*/
/// New "act as you" requests the token file may make in an hour (review round 3, R3-2).
pub const OWNER_KEY_REQUESTS_PER_HOUR: usize = 12;

/// Compares two hashes without stopping at the first difference.
fn same_bytes(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0_u8, |differ, (a, b)| differ | (a ^ b))
            == 0
}

/// The file the app writes its one-time value to just before it starts gxserver (in `auth/`).
pub const FIRST_KEY_HANDOFF_FILE: &str = "first-key-handoff";

/// How long one Approve lets the token file get pairing codes (`ghostex computer-code`'s idle limit).
pub const TOKEN_FILE_CODE_WINDOW_MS: u64 = 10 * 60 * 1000;

/// The data an owner-key request keeps (`ApprovalStore::attach`): the key and its scope
/// (`doorOnly`), so gxserver lets the key in the moment the owner approves.
pub const OWNER_KEY_RESUME: &str = "ownerKey";

/*
CDXC:Security 2026-11-16 FIX:
Review M2 of the owner-only door: the session the app opens for the door was a full owner session (every capability, secrets included, no end), while the card that lets it in says it shows codes and approves devices. The app now asks for a door session (`scope: "door"` on `/api/openOwnerSession`): it is never in the owner session table, so no route, socket or relay but the door's own (`DOOR_SESSION_ENDPOINTS`) knows it, and it lapses after `DOOR_SESSION_TTL_MS` (the app opens a new one, no card: the key is enrolled). A key enrolled with `scope: "door"` (this app's keys, and a key let in through its card) opens only door sessions. An app from before this (11.2.2 and earlier keep their owner session for every call) enrolls without a scope and opens a full session as before; its card says that it acts as you for everything here.
*/
/// How long a door session works before the app opens another.
pub const DOOR_SESSION_TTL_MS: u64 = 12 * 60 * 60 * 1000;
/// The only routes a door session opens (review M2 above): pairing codes, approvals and paired
/// devices, and the signed ask that has another computer connect back here.
pub const DOOR_SESSION_ENDPOINTS: &[&str] = &[
    "/api/remotePairingCode",
    "/api/pairedDevices",
    "/api/removePairedDevice",
    "/api/signComputerConnectBack",
    super::APPROVALS_LIST_ENDPOINT,
    super::APPROVALS_READ_ENDPOINT,
    super::APPROVALS_APPROVE_ENDPOINT,
    super::APPROVALS_DENY_ENDPOINT,
    super::SECURITY_STATUS_ENDPOINT,
];

/// Whether `action` lets someone in or makes an owner: approved only by the door's keepers.
pub fn opens_door(action: &SensitiveAction) -> bool {
    matches!(
        action,
        SensitiveAction::PairingApproveDevice
            | SensitiveAction::PairingShowCode
            | SensitiveAction::AddApproverKey
    )
}

impl SecurityState {
    /// Whether this computer has an owner device: an enrolled owner key, or a phone paired with
    /// its own token (now or before).
    pub fn has_owner_device(&self) -> bool {
        !self.owner_keys.list().is_empty() || self.is_remembered(OWNER_DEVICE)
    }

    /// Whether owner key `key_id` opens door sessions only (review M2 above).
    pub fn is_door_only_key(&self, key_id: &str) -> bool {
        self.door_only_keys
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains(key_id)
    }

    /// Owner key `key_id` opens door sessions only from now on.
    pub fn mark_door_only_key(&self, key_id: &str) {
        let added = self
            .door_only_keys
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(key_id.to_string());
        if added {
            let _ = self.save();
        }
    }

    /// A phone paired with its own token is here: remembered for good.
    pub fn note_owner_device(&self, now_ms: u64) {
        if !self.is_remembered(OWNER_DEVICE) {
            self.remember(OWNER_DEVICE, now_ms);
        }
    }

    /// Who makes pairing codes and approves new devices and owners: the app's owner session and
    /// paired phones, and the token file only while this computer has no owner device.
    pub fn keeps_door(&self, caller: &Principal) -> bool {
        match caller {
            Principal::Owner { .. } | Principal::Device { .. } => true,
            Principal::LocalAgent => !self.has_owner_device(),
            _ => false,
        }
    }

    /// Keeps the one-time value the app handed over as it started gxserver (`FIRST_KEY_HANDOFF_FILE`).
    pub fn set_first_key_secret(&self, secret: &str) {
        use sha2::{Digest, Sha256};
        *self
            .first_key_secret_hash
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
            Some(Sha256::digest(secret.trim().as_bytes()).to_vec());
    }

    /// Whether the token file may add an owner key without a card now (review round 2, R2-3
    /// below): only while this computer has no owner key and no paired phone, and only with the
    /// one-time value the app handed gxserver as it started it, which this spends.
    pub fn take_first_key(&self, presented: Option<&str>) -> bool {
        use sha2::{Digest, Sha256};
        if self.has_owner_device() {
            return false;
        }
        let Some(presented) = presented.map(str::trim).filter(|value| !value.is_empty()) else {
            return false;
        };
        let mut stored = self
            .first_key_secret_hash
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let matches = stored
            .as_deref()
            .is_some_and(|hash| same_bytes(hash, &Sha256::digest(presented.as_bytes())));
        if matches {
            *stored = None;
        }
        matches
    }

    /// Whether `presented` is the one-time value the app handed over at this start, without
    /// spending it (review round 3, R3-2: the app's own "act as you" request is never capped or
    /// pushed out).
    pub fn is_first_key_secret(&self, presented: Option<&str>) -> bool {
        use sha2::{Digest, Sha256};
        let Some(presented) = presented.map(str::trim).filter(|value| !value.is_empty()) else {
            return false;
        };
        self.first_key_secret_hash
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_deref()
            .is_some_and(|hash| same_bytes(hash, &Sha256::digest(presented.as_bytes())))
    }

    /// Notes a new "act as you" request from the token file; `false` when the hour's cap
    /// (`OWNER_KEY_REQUESTS_PER_HOUR`) is reached, so it isn't made.
    pub fn note_owner_key_request(&self, now_ms: u64) -> bool {
        let mut recent = self
            .owner_key_requests
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        recent.retain(|at| at + 60 * 60 * 1000 > now_ms);
        if recent.len() >= OWNER_KEY_REQUESTS_PER_HOUR {
            return false;
        }
        recent.push(now_ms);
        true
    }

    /// The app's own "act as you" request here (it carried this start's value): never pushed out.
    pub fn note_app_owner_key_request(&self, id: &str) {
        self.app_owner_key_requests
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(id.to_string());
    }

    /// Whether request `id` is the app's own (see [`SecurityState::note_app_owner_key_request`]).
    pub fn is_app_owner_key_request(&self, id: &str) -> bool {
        self.app_owner_key_requests
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains(id)
    }

    /// The token file was just shown a pairing code (a terminal's `ghostex computer-code`).
    pub fn open_token_file_code_window(&self, now_ms: u64) {
        self.set_window(
            &self.token_file_code_until,
            now_ms + TOKEN_FILE_CODE_WINDOW_MS,
        );
    }

    /// Whether the token file was shown a pairing code within [`TOKEN_FILE_CODE_WINDOW_MS`].
    pub fn token_file_code_shown_lately(&self, now_ms: u64) -> bool {
        now_ms < self.window(&self.token_file_code_until)
    }

    /// Whether this computer's own app is its only owner: owner keys there are all ones its own
    /// app enrolled with the one-time value it hands gxserver (`appOwnerKeys`), and no phone ever
    /// paired with its own token.
    pub fn only_own_app_owns(&self) -> bool {
        let keys = self.listed_owner_keys();
        !keys.is_empty()
            && !self.is_remembered(OWNER_DEVICE)
            && keys.iter().all(|key| self.is_app_owner_key(&key.key_id))
    }

    /*
    CDXC:Security 2026-11-17 DECISION:
    User (Q-4B, on a headless computer whose only owner is its own Ghostex app, which they never look at): "that app hands ownership to the first computer that joins it through ghostex computer-code (after the user's Approve of the join, with the matching words). Keep it as tight as possible: only the first join through computer-code, only while that computer has no other owner besides its own app, never through an agent, logged in the security log in plain words, and the new owner is told." The token file (the user's terminal; an agent in a Ghostex session holds a session token instead, which never gets a code or approves) may approve here only while (1) this computer's own app is its only owner (`only_own_app_owns`), and (2) a terminal was shown a pairing code in the last ten minutes, and only (a) a computer (never a phone) joining with that code, then (b) within ten minutes of that join, that same computer's app asking to manage this one (its door-only owner key), which gxserver lets in at the click (CDXC:Security 2026-11-17 at `enroll_owner_key_on_approval`). Once (b) is in, this computer has another owner and (1) no longer holds, so the terminal approves nothing again. A program that reads the token file itself (not through a Ghostex session) is the user's terminal here, as everywhere the token file decides (W524).
    */
    /// Whether the token file may approve `request` here: CDXC:Security 2026-11-17 DECISION above.
    pub fn terminal_may_approve(
        &self,
        caller: &Principal,
        request: &ApprovalRequest,
        now_ms: u64,
    ) -> bool {
        if !matches!(caller, Principal::LocalAgent)
            || !self.only_own_app_owns()
            || !self.token_file_code_shown_lately(now_ms)
        {
            return false;
        }
        let summary = request.summary.trim();
        match request.action {
            // A computer joining (gxserver writes "Let a <system> computer that calls itself …";
            // a phone's starts "Pair "), and only the first one this run: approving another join,
            // or the same code shown again, doesn't open a second join or restart the ten minutes
            // of the ask below (review of 8abb0a8db).
            SensitiveAction::PairingApproveDevice => {
                summary.starts_with("Let a ") && !self.terminal_join_used()
            }
            // The computer that joined asking to manage this one: a door-only ask made after that
            // join, within its ten minutes (an app names itself by its host name, which can differ
            // from the name it joined with, so the words the user matches are the check). Door-only
            // is read from the key's scope kept with the request, not from the summary, which
            // carries the asking app's own label (an "act as you" ask whose label says "show the
            // pairing code of" must not pass, review of 8abb0a8db).
            SensitiveAction::AddApproverKey => {
                self.approvals
                    .resume_of(&request.id, OWNER_KEY_RESUME)
                    .is_some_and(|saved| saved["doorOnly"].as_bool() == Some(true))
                    && self
                        .terminal_join_at(now_ms)
                        .is_some_and(|at| request.created_at_ms >= at)
            }
            _ => false,
        }
    }

    /// Whether the terminal already let a computer in during this run (it lets in only one).
    pub fn terminal_join_used(&self) -> bool {
        self.terminal_join_used
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// The terminal let computer `name` in (request `id`).
    pub fn note_terminal_join(&self, id: &str, name: &str, now_ms: u64) {
        self.terminal_join_used
            .store(true, std::sync::atomic::Ordering::SeqCst);
        *self
            .terminal_join
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
            Some((id.to_string(), name.to_string(), now_ms));
    }

    /// The computer the terminal let in within [`TOKEN_FILE_CODE_WINDOW_MS`]: (request id, name).
    pub fn terminal_join(&self, now_ms: u64) -> Option<(String, String)> {
        self.terminal_join
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .filter(|(_, _, at)| now_ms < at + TOKEN_FILE_CODE_WINDOW_MS)
            .map(|(id, name, _)| (id, name))
    }

    /// When the terminal let a computer in, within [`TOKEN_FILE_CODE_WINDOW_MS`].
    pub fn terminal_join_at(&self, now_ms: u64) -> Option<u64> {
        self.terminal_join
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .map(|(_, _, at)| *at)
            .filter(|at| now_ms < at + TOKEN_FILE_CODE_WINDOW_MS)
    }

    /// The handover is done (or given up): the terminal approves nothing more.
    pub fn clear_terminal_join(&self) {
        *self
            .terminal_join
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }

    fn window(&self, until: &std::sync::Mutex<u64>) -> u64 {
        *until
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn set_window(&self, until: &std::sync::Mutex<u64>, to: u64) {
        *until
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = to;
    }
}

/*
CDXC:Security 2026-11-10 DECISION:
User (Q2140 = B, after the T3 Code comparison page): "thinking of removing all these dumb blocks because they're adding friction and a lot of bugs. app was way simpler before. i dont know why we want to ban agents from doing everything with ghostex but after testing it this seems like it's the wrong direction", and on the answer: "add a toggle to require touch id/pin etc for access but disabled by default, add it to security". So the door stays and nothing else asks:
- Only what lets someone new in asks, with one plain Approve / Deny card (no Touch ID, no signature, `/api/approveApprovalRequest` with just `{ id }`): pairing a phone or a computer (the three words stay), and each Collab guest message in Approve mode, whoever asks; and, when an agent's session token asks, turning Easy Connect on (once, remembered), adding a device to its allow-list, a new Collab share or its link again, and Send mode for a share. Your own click in Ghostex (the owner session, the token file, a paired phone) is the approval for your own action, so it never shows a card. Answering a secret request stays yours alone (`Capability::OwnerSecrets`).
- Only you approve: the owner session, the token file (the desktop app, the web build, the CLI in your terminal) and paired phones (`Capability::DecideApprovals`); an agent's session token can't, and a request never approves itself.
- Everything else just works for you, your devices and your agents: connecting computers, settings, GBots, Autopilot, Cloud Boxes, teams, full access, extensions, and steering other sessions.
- "Require Touch ID, Windows Hello or your password to approve" (`approvalsRequirePresence`, Settings > Security, off by default) is a check the approving device makes itself before it sends the same plain approval; gxserver only keeps the switch for every device that approves for this computer (`/api/securityStatus`).
- Older apps keep working: a signed approval counts as a plain one, approver and owner keys enroll without asking, and gxserver says `plainApprovals` so newer apps stop signing.
This supersedes the per-action answers of earlier the same day (Q2098 to Q2110) and the 8 Nov rules.
*/
/*
CDXC:Chief 2026-11-08 DECISION:
    User (Q1749, Q1750): Chief gets "a head role granted once" with no Touch ID: it reads every session and types into, interrupts and messages any session freely, and answers other agents' approval prompts without asking ("prompts free"). "We already don't allow agents to manage stuff related to collab/access/pairing/security, agents should just point to the settings for these", so every other sensitive action still asks, for Chief exactly as for any agent. `SecurityState::skips_approval` (security/policy.rs) is the one place that says which approvals a caller skips.
*/
/*
CDXC:Security 2026-11-08 DECISION:
    User, after 11.1.2: "please let agents manage other sessions like before. no need to limit this." Answering another session's permission prompt, typing into it and interrupting it ask nobody again, as before 11.1.x: agents here, the token file, and agents on a connected computer within what its grant allows. Chief's are still written to the security log as `allowed` (`security_gates::record_allowed`).
*/
impl SecurityState {
    /// Whether `caller` may do `action` without a card (see the decision above).
    pub fn skips_approval(&self, caller: &Principal, action: &ActionRequest) -> bool {
        use ghostex_gx_protocol::approval::{asks_when, AsksWhen};
        // A pairing code for the token file, and another owner key: only the door's keepers
        // (CDXC:Security 2026-11-16 below).
        if matches!(
            action.action,
            SensitiveAction::PairingShowCode | SensitiveAction::AddApproverKey
        ) {
            return self.keeps_door(caller);
        }
        let you = caller.is_owner_device();
        // The list Settings > Security shows is the rule (`approval::STILL_ASKS`).
        match asks_when(&action.action) {
            // Someone new asks to come in: always a card.
            Some(AsksWhen::Always) => false,
            // Turning Easy Connect on asks an agent once, then is remembered.
            Some(AsksWhen::AgentAsks) if action.action == SensitiveAction::EasyConnectEnable => {
                you || self.is_remembered(EASY_CONNECT_ENABLE)
            }
            // Lets someone new in when an agent does it; your own click is the approval.
            Some(AsksWhen::AgentAsks) => you,
            None => true,
        }
    }

    /// Keeps an approval that is asked once and remembered (turning Easy Connect on, when an agent asks).
    pub fn remember_approved(&self, request: &ApprovalRequest, now_ms: u64) {
        if let Some(key) = remember_key(request) {
            self.remember(key, now_ms);
        }
    }
}

/// The remembered approval for turning Easy Connect on.
pub const EASY_CONNECT_ENABLE: &str = "easyConnectEnable";

fn remember_key(request: &ApprovalRequest) -> Option<&'static str> {
    (request.action == SensitiveAction::EasyConnectEnable).then_some(EASY_CONNECT_ENABLE)
}
