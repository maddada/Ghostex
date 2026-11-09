//! Signed approvals for sensitive actions: the wire contract the desktop app, the phone and
//! gxserver share.
//!
//! An agent (or any caller) can only *ask* for a sensitive action. gxserver answers the call
//! with an `approvalRequired` error carrying an [`ApprovalRequest`]; the request shows on the
//! computer and the phone; a device that holds an approver key signs
//! [`approval_signing_payload`] after Touch ID / Windows Hello / polkit / the phone's biometric
//! prompt and posts it to `/api/approveApprovalRequest`; the caller then repeats its call with
//! [`APPROVAL_ID_PARAM`] set to the request's id. The approval is single-use and bound to the
//! exact action, target and parameters, so it cannot be spent on a different change.
//!
//! Full design: `ai/security-tokens-and-approvals.md`.

use serde::{Deserialize, Serialize};

/// The `error` code of a call that needs the owner's signed approval first.
pub const APPROVAL_REQUIRED_ERROR: &str = "approvalRequired";
/// The `error` code of a call whose approval request the owner denied.
pub const APPROVAL_DENIED_ERROR: &str = "approvalDenied";
/// The body key a caller repeats its call with once its request was approved.
pub const APPROVAL_ID_PARAM: &str = "approvalId";
/// First line of every signed approval payload; bump the version to change the format.
pub const APPROVAL_SIGNING_DOMAIN: &str = "ghostex-approval-v1";

open_string_enum! {
    /// Every action that needs the owner's signed approval (GitHub #495, decisions of
    /// 2026-11-08). Turning something off, stopping a share, removing a viewer and revoking a
    /// device are never in this list: making Ghostex more closed is always allowed.
    SensitiveAction {
        // Collab
        /// Turn Ghostex Collab on.
        CollabEnable => "collabEnable",
        /// Share a chat through a Collab link.
        CollabShareChat => "collabShareChat",
        /// Change a share's mode, code or expiry (Send mode has its own action).
        CollabChangeShare => "collabChangeShare",
        /// Turn on Send mode for a share (a guest types into the agent without approval).
        CollabEnableSendMode => "collabEnableSendMode",
        /// Approve a guest's message in Approve mode.
        CollabApproveGuestMessage => "collabApproveGuestMessage",
        /// Change the Collab relay.
        CollabChangeRelay => "collabChangeRelay",
        // Pairing
        /// Make a pairing QR / code on the desktop's Show QR sheet.
        PairingShowCode => "pairingShowCode",
        /// Approve a Pending device after the three words matched.
        PairingApproveDevice => "pairingApproveDevice",
        // Easy Connect
        /// Turn Easy Connect on.
        EasyConnectEnable => "easyConnectEnable",
        /// Add to or widen Easy Connect's allow-list.
        EasyConnectAllowList => "easyConnectAllowList",
        /// Open or change Easy Connect's forwarded ports.
        EasyConnectPorts => "easyConnectPorts",
        /// Change Easy Connect's relay.
        EasyConnectRelay => "easyConnectRelay",
        // Connected computers
        /// Register a connected computer.
        ComputerRegister => "computerRegister",
        /// Issue (or renew) a grant that lets another computer's agents reach this one.
        ComputerGrant => "computerGrant",
        /// Widen what a connected computer may do here.
        ComputerChangeAccess => "computerChangeAccess",
        // Autopilot
        /// Let Autopilot answer approval prompts (or widen what it may answer).
        AutopilotAnswerApprovals => "autopilotAnswerApprovals",
        /// Lift or raise Autopilot's hard limits.
        AutopilotLiftLimits => "autopilotLiftLimits",
        // GBots
        /// Turn GBots on.
        GbotsEnable => "gbotsEnable",
        /// Add a bot.
        GbotsAddBot => "gbotsAddBot",
        /// Turn on a GBot's API, or give it a new key (the target is the GBot's id): an app or
        /// script holding the key can message it.
        GbotsApiKey => "gbotsApiKey",
        /// Let a GBot run its chats on another computer or a Cloud Box, in a new folder there, or
        /// use the screen and apps (the target is the GBot's id).
        GbotsSetComputer => "gbotsSetComputer",
        // Settings
        /// Change a setting marked security-relaxing (the target is the setting key).
        SettingsWeakenSecurity => "settingsWeakenSecurity",
        // Prompts. These three never ask since the user's "let agents manage other sessions like
        // before" (11.1.2); gxserver still names them in Chief's security log entries.
        /// Answer an approval prompt of a session that is not the caller's own thread.
        AnswerOtherSessionPrompt => "answerOtherSessionPrompt",
        /// Type raw terminal input (text, Enter, a key) into a session that is not the caller's
        /// own, its threads included; a coordinator interrupting its own thread doesn't ask.
        TypeIntoOtherSession => "typeIntoOtherSession",
        /// Stop what a session that is neither the caller's own nor one of its threads is doing
        /// (`interrupt-session-chat`).
        InterruptOtherSession => "interruptOtherSession",
        // Approvers
        /// Add another approver key (a second desktop or a phone) after the first one.
        AddApproverKey => "addApproverKey",
        // Extensions
        /// Download and install an extension component that opens a way into this computer
        /// (the target is the component's name, such as `collab`). Updating an installed one,
        /// turning it off and uninstalling it never ask.
        InstallExtensionComponent => "installExtensionComponent",
        // Other projects (#495 follow-up). `reachOtherProject` (11.1.1 and 11.1.2: an agent's
        // first message to, or new session in, another project) is retired: agents reach any
        // project freely, and an old audit entry naming it reads as `Other`.
        /// A create call that starts an agent with approvals off (`acceptAllMode: "enabled"`)
        /// when the owner's own setting for it doesn't. Asks every time.
        ForceFullAccess => "forceFullAccess",
        // Cloud Boxes
        /// Start a box, or change what boxes get, when that puts the owner's agent logins or a
        /// project's secret files onto a box (`/api/cloudBoxes` `create`, `setAgentLogin` with
        /// `allowed: true`, `projectSettings` with `settings`).
        CloudBoxShareLogins => "cloudBoxShareLogins",
        // Teams
        /// Connect a workspace to a team backend with a token made elsewhere
        /// (`ghostex team deploy`'s last step), by anyone but the owner's devices; the target is
        /// the backend's address. Joining, invites and the team's keys are the owner's alone.
        TeamSyncConnect => "teamSyncConnect",
    }
}

open_string_enum! {
    /// Where an [`ApprovalRequest`] stands.
    ApprovalStatus {
        /// Waiting for the owner.
        Pending => "pending",
        /// Signed by an approver key; the caller may repeat its call once.
        Approved => "approved",
        /// The owner said no.
        Denied => "denied",
        /// Nobody answered in time, or an approval went unused in time.
        Expired => "expired",
        /// The approval was spent on the call it was made for.
        Consumed => "consumed",
    }
}

open_string_enum! {
    /// The signature scheme of an approver key. Both are what the platforms' hardware-backed
    /// keys produce: P-256 from the Secure Enclave (macOS, iOS) and Android Keystore, RSA from
    /// Windows Hello's `KeyCredentialManager`.
    ApproverKeyAlgorithm {
        /// ECDSA over P-256 with SHA-256; public key is the 65-byte uncompressed point (X9.63),
        /// signature is ASN.1 DER.
        EcdsaP256Sha256 => "ecdsaP256Sha256",
        /// RSASSA-PKCS1-v1_5 with SHA-256, 2048 bits or more; public key is PKCS#1
        /// `RSAPublicKey` DER.
        RsaPkcs1Sha256 => "rsaPkcs1Sha256",
    }
}

open_string_enum! {
    /// What unlocks an approver key on its device, shown next to the key in Settings.
    ApproverPresence {
        TouchId => "touchId",
        MacPassword => "macPassword",
        WindowsHello => "windowsHello",
        WindowsPassword => "windowsPassword",
        Polkit => "polkit",
        FaceId => "faceId",
        Biometric => "biometric",
        DevicePin => "devicePin",
    }
}

/// Who asked for an approval, as the approval card shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequester {
    /// `owner`, `device`, `session`, `localAgent`, `peer`, `extension`.
    pub kind: String,
    /// The session's title, the device's or computer's name; written by gxserver.
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
}

/// One request for the owner's approval.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub id: String,
    pub action: SensitiveAction,
    /// The chat, device, computer or setting key the action is about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// One line written by gxserver from the action and its parameters, never by the caller.
    pub summary: String,
    /// Optional second line (for example the new value of a setting), also written by gxserver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Lowercase hex SHA-256 of the canonical JSON of the call's parameters.
    pub params_digest: String,
    /// 32 random bytes, base64url without padding.
    pub nonce: String,
    pub requested_by: ApprovalRequester,
    pub created_at_ms: u64,
    /// While pending: when the request lapses. Once approved: when the approval lapses unused.
    pub expires_at_ms: u64,
    pub status: ApprovalStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at_ms: Option<u64>,
    /// The approver key that signed it (approved), or the device that denied it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_by: Option<String>,
}

/// An approver key as Settings > Security lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApproverKey {
    /// Lowercase hex of the first 16 bytes of SHA-256 over the public key bytes.
    pub key_id: String,
    pub device_id: String,
    pub label: String,
    /// `macos`, `windows`, `linux`, `ios`, `android`.
    pub platform: String,
    pub algorithm: ApproverKeyAlgorithm,
    /// The public key bytes (see [`ApproverKeyAlgorithm`]), base64 standard with padding.
    pub public_key: String,
    pub presence: ApproverPresence,
    pub added_at_ms: u64,
}

/// `/api/readApprovalRequest`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadApprovalParams {
    pub id: String,
}

/// `/api/approveApprovalRequest`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApproveApprovalParams {
    pub id: String,
    pub key_id: String,
    /// The signature over [`approval_signing_payload`], base64 standard with padding.
    pub signature: String,
}

/// `/api/denyApprovalRequest`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DenyApprovalParams {
    pub id: String,
}

/// Why [`approval_signing_payload`] refused to build a payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SigningPayloadError {
    pub field: &'static str,
}

impl std::fmt::Display for SigningPayloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "approval field {} is empty or contains a line break",
            self.field
        )
    }
}

impl std::error::Error for SigningPayloadError {}

/// The exact bytes an approver key signs to approve `request` on the gxserver `server_id`.
///
/// One field per line, UTF-8, `\n` line ends, no trailing newline:
///
/// ```text
/// ghostex-approval-v1
/// <server id>
/// <approval id>
/// <nonce>
/// <action>
/// <target, or empty>
/// <params digest>
/// <expiresAtMs while pending, decimal>
/// approve
/// ```
///
/// Line-based on purpose: no JSON canonicalization for the phone's TypeScript and the
/// desktop's Rust to disagree on. A field with a line break is refused, so no field can forge
/// another line.
pub fn approval_signing_payload(
    server_id: &str,
    request: &ApprovalRequest,
) -> Result<Vec<u8>, SigningPayloadError> {
    let target = request.target.as_deref().unwrap_or("");
    let expires = request.expires_at_ms.to_string();
    let fields: [(&'static str, &str, bool); 7] = [
        ("serverId", server_id, true),
        ("id", &request.id, true),
        ("nonce", &request.nonce, true),
        ("action", request.action.as_str(), true),
        ("target", target, false),
        ("paramsDigest", &request.params_digest, true),
        ("expiresAtMs", &expires, true),
    ];
    let mut payload = String::from(APPROVAL_SIGNING_DOMAIN);
    for (field, value, required) in fields {
        if value.contains(['\n', '\r']) || (required && value.is_empty()) {
            return Err(SigningPayloadError { field });
        }
        payload.push('\n');
        payload.push_str(value);
    }
    payload.push_str("\napprove");
    Ok(payload.into_bytes())
}

/// First line of the payload the desktop's owner key signs to open an owner session.
pub const OWNER_SESSION_SIGNING_DOMAIN: &str = "ghostex-owner-session-v1";

/// `/api/ownerChallenge` → a one-time challenge the owner key signs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerChallenge {
    pub challenge: String,
    pub expires_at_ms: u64,
}

/// `/api/openOwnerSession`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenOwnerSessionParams {
    pub key_id: String,
    pub challenge: String,
    /// The signature over [`owner_session_signing_payload`], base64 standard with padding.
    pub signature: String,
}

/// `/api/openOwnerSession` → the owner session token, held only in the desktop app's memory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenOwnerSessionResult {
    pub token: String,
    pub device_id: String,
}

/// `/api/enrollOwnerKey` and `/api/enrollApproverKey`: the public key to add. gxserver
/// recomputes `keyId` and sets `addedAtMs`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollKeyParams {
    pub key: ApproverKey,
}

/// The exact bytes the owner key signs to open an owner session on gxserver `server_id`:
/// `ghostex-owner-session-v1\n<server id>\n<challenge>\n<key id>`.
pub fn owner_session_signing_payload(
    server_id: &str,
    challenge: &str,
    key_id: &str,
) -> Result<Vec<u8>, SigningPayloadError> {
    let fields: [(&'static str, &str); 3] = [
        ("serverId", server_id),
        ("challenge", challenge),
        ("keyId", key_id),
    ];
    let mut payload = String::from(OWNER_SESSION_SIGNING_DOMAIN);
    for (field, value) in fields {
        if value.is_empty() || value.contains(['\n', '\r']) {
            return Err(SigningPayloadError { field });
        }
        payload.push('\n');
        payload.push_str(value);
    }
    Ok(payload.into_bytes())
}
