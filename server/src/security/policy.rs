//! Which sensitive actions skip the owner's signed approval, and which approvals are remembered
//! once given: the one rule both gates read (`security_gates::gate` for whole routes, and
//! [`SecurityState::require_owner_presence`] for the handlers that ask themselves).

use ghostex_gx_protocol::approval::{ApprovalRequest, SensitiveAction};

use super::{ActionRequest, Principal, SecurityState};

/*
CDXC:Security 2026-11-10 DECISION:
User, on the page listing every fingerprint approval ("i'm asking about 'permissions' where you need to confirm using finger print … things are starting to break due to those"), group by group:
- Q2098 connected computers, drop for computers you add yourself ("Drop it for computers we add please."): letting a computer's agents in never asks your own app (its owner session, its SSH login's token file) or your phone; anything else (an agent's own token, another computer's agents) still asks.
- Q2099 another computer or phone becoming an approver, ask once and remember: unchanged here (one card, the keys are kept for good); the desktop asks at connect instead of waiting for you to act there (owner_security `cached_token`).
- Q2100 pairing, "keep, but never for a phone that signs in over SSH": not done; it would let any agent on the computer pair itself as an owner device that approves, which the user's "keep the protections you didn't drop" rules out. Asked back to the user; pairing asks once per device as before.
- Q2101 Easy Connect, ask only for turning it on: turning it on asks once and is remembered (a computer that already had Easy Connect set up counts as approved); ports, relay and allow-list changes don't ask.
- Q2102 settings, ask only for settings that let someone in (`ghostex_settings_catalog::security_relaxing`).
- Q2103 Collab, ask once per share: your own Share popover shows a share's link again and switches its mode without asking; a new share and each guest message in Approve mode still ask; agents still ask for all of it.
- Q2104 agents' bigger moves, drop: full access at start, Autopilot answering approvals and its limits, Cloud Box logins and project files, and connecting a team backend never ask; each is written to the security log as allowed.
- Q2105 GBots, drop for your own clicks: turning GBots on, adding a GBot, its API key and where it runs don't ask from your computer or phone; the terminal and agents still ask (most are refused by `/api/gbots` anyway).
Steering another session (answering its prompt, typing into it, stopping it) never asks anyone (11.1.2, below). Only a person approves what still asks; agents can't approve; a session token can't enroll an owner.
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
    /// Whether `caller` may do `action` without asking the owner (see the decision above).
    pub fn skips_approval(&self, caller: &Principal, action: &ActionRequest) -> bool {
        use SensitiveAction::*;
        let own_device = caller.is_owner_device();
        match &action.action {
            AnswerOtherSessionPrompt | TypeIntoOtherSession | InterruptOtherSession => true,
            // Q2098: your own app (its owner session here, or the token file its SSH login reads
            // on the other computer) and your phone.
            ComputerGrant | ComputerRegister | ComputerChangeAccess => {
                own_device || matches!(caller, Principal::LocalAgent)
            }
            // Q2101.
            EasyConnectAllowList | EasyConnectPorts | EasyConnectRelay => true,
            EasyConnectEnable => self.is_remembered(EASY_CONNECT_ENABLE),
            // Q2104.
            ForceFullAccess
            | AutopilotAnswerApprovals
            | AutopilotLiftLimits
            | CloudBoxShareLogins
            | TeamSyncConnect => true,
            // Q2105.
            GbotsEnable | GbotsAddBot | GbotsApiKey | GbotsSetComputer => own_device,
            // Q2103: the share was approved when it was made; its link again and its mode are
            // your own call. The component writes this summary for a share that already exists
            // (collab/src/owner.rs), never the caller.
            CollabEnableSendMode | CollabChangeShare => own_device,
            CollabShareChat => own_device && action.summary.starts_with(SHOW_SHARE_AGAIN),
            _ => false,
        }
    }

    /// The Q2104 actions, which the security log keeps as `allowed` since they no longer ask.
    pub fn logs_when_skipped(action: &SensitiveAction) -> bool {
        use SensitiveAction::*;
        matches!(
            action,
            ForceFullAccess
                | AutopilotAnswerApprovals
                | AutopilotLiftLimits
                | CloudBoxShareLogins
                | TeamSyncConnect
        )
    }

    /// Keeps an approval that is asked once and remembered (Q2101: turning Easy Connect on).
    pub fn remember_approved(&self, request: &ApprovalRequest, now_ms: u64) {
        if let Some(key) = remember_key(request) {
            self.remember(key, now_ms);
        }
    }
}

/// The remembered approval for turning Easy Connect on.
pub const EASY_CONNECT_ENABLE: &str = "easyConnectEnable";

/// How the Collab component's card starts for a share that already exists.
const SHOW_SHARE_AGAIN: &str = "Show the link and code for ";

fn remember_key(request: &ApprovalRequest) -> Option<&'static str> {
    (request.action == SensitiveAction::EasyConnectEnable).then_some(EASY_CONNECT_ENABLE)
}
