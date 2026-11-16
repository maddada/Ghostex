//! Which sensitive actions skip the owner's signed approval, and which approvals are remembered
//! once given: the one rule both gates read (`security_gates::gate` for whole routes, and
//! [`SecurityState::require_owner_presence`] for the handlers that ask themselves).

use ghostex_gx_protocol::approval::{ApprovalRequest, SensitiveAction};

use super::{ActionRequest, Principal, SecurityState};

/*
CDXC:Security 2026-11-16 DECISION:
User (W524, "Owner only"): only the Ghostex app on this computer or a paired phone makes pairing codes and approves new devices; agents running as the user must not. The token file is readable by every program of the user (its path is in every session's environment), so once this computer has an owner device (the app's owner key, kept in the OS key store, is enrolled, or a phone paired with its own token), the token file alone no longer:
- gets a pairing code without an Approve on the app or a phone (`ghostex computer-code` asks once, then gets codes for ten minutes, `TOKEN_FILE_CODE_WINDOW_MS`);
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
/// How long one Approve lets the token file get pairing codes (`ghostex computer-code`'s idle limit).
pub const TOKEN_FILE_CODE_WINDOW_MS: u64 = 10 * 60 * 1000;

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

    /// Whether the token file may add an owner key without a card now: only while this computer
    /// has no owner key and no paired phone (the first key; review B2 above).
    pub fn token_file_may_add_owner_key(&self) -> bool {
        !self.has_owner_device()
    }

    /// One Approve of a terminal's pairing code: the token file gets codes for a while.
    pub fn open_token_file_code_window(&self, now_ms: u64) {
        self.set_window(
            &self.token_file_code_until,
            now_ms + TOKEN_FILE_CODE_WINDOW_MS,
        );
    }

    /// Whether the token file gets a pairing code now: it keeps the door, or one was approved
    /// within [`TOKEN_FILE_CODE_WINDOW_MS`].
    pub fn token_file_gets_code(&self, now_ms: u64) -> bool {
        self.keeps_door(&Principal::LocalAgent) || now_ms < self.window(&self.token_file_code_until)
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
