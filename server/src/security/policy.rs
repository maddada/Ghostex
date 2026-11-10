//! Which sensitive actions skip the owner's signed approval, and which approvals are remembered
//! once given: the one rule both gates read (`security_gates::gate` for whole routes, and
//! [`SecurityState::require_owner_presence`] for the handlers that ask themselves).

use ghostex_gx_protocol::approval::{ApprovalRequest, SensitiveAction};

use super::{ActionRequest, Principal, SecurityState};

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
