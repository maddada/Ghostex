//! The open string enums of the Cloud Boxes contract.

open_string_enum! {
    /// Which service runs a box. `local` is the zero-cost development provider: a Docker container
    /// on this computer running the Linux gxserver package; it is listed only with Show debug UI
    /// controls on (or `GHOSTEX_CLOUD_BOXES_LOCAL_PROVIDER=1`) and never bills.
    CloudProviderId {
        Instabox => "instabox",
        Boat => "boat",
        Local => "local",
    }
}

impl CloudProviderId {
    /// The provider name a user reads.
    pub fn label(&self) -> &str {
        match self {
            Self::Instabox => "Instabox",
            Self::Boat => "Boat",
            Self::Local => "Local (Docker)",
            Self::Other(value) => value.as_str(),
        }
    }
}

open_string_enum! {
    /// Where a box is in its life. Transitions are owned by `packages/gx-cloud` (`state.rs`).
    ///
    /// - `pending`: a draft picked Cloud Box; nothing exists at the provider and nothing bills.
    /// - `creating`: the provider is making the machine.
    /// - `booting`: the machine exists; `ghostex box boot` and session preparation are running.
    /// - `running`: gxserver on the box is reachable.
    /// - `idle`: running with no agent turn and no open terminal; the idle countdown is on.
    /// - `paused`: Instabox suspended it (programs kept). Sending wakes it.
    /// - `stopped`: stopped or archived (Boat: programs ended, disk kept). Sending wakes it.
    /// - `deleting` / `deleted`: delete requested / done (the row is kept for the history).
    /// - `failed`: a create, boot or resume step failed; `stateDetail` says why.
    CloudBoxState {
        Pending => "pending",
        Creating => "creating",
        Booting => "booting",
        Running => "running",
        Idle => "idle",
        Paused => "paused",
        Stopped => "stopped",
        Deleting => "deleting",
        Deleted => "deleted",
        Failed => "failed",
    }
}

impl CloudBoxState {
    /// The box is up and a client can talk to its gxserver.
    pub fn is_reachable(&self) -> bool {
        matches!(self, Self::Running | Self::Idle)
    }

    /// Sending a message must resume the box first (Q1013: wake on send, never on open).
    pub fn wakes_on_send(&self) -> bool {
        matches!(self, Self::Paused | Self::Stopped)
    }

    /// Still on its way up: the progress card is showing.
    pub fn is_starting(&self) -> bool {
        matches!(self, Self::Pending | Self::Creating | Self::Booting)
    }

    /// Nothing more will happen to the box.
    pub fn is_gone(&self) -> bool {
        matches!(self, Self::Deleted)
    }
}

impl Default for CloudBoxState {
    fn default() -> Self {
        Self::Pending
    }
}

open_string_enum! {
    /// Whether Ghostex can call a provider for the user.
    CloudProviderConnectionState {
        NotConnected => "notConnected",
        /// A browser sign-in is waiting for the user (see `connectStatus`).
        Connecting => "connecting",
        Connected => "connected",
        /// The saved key or refresh token stopped working; connect again.
        Expired => "expired",
        Error => "error",
    }
}

impl Default for CloudProviderConnectionState {
    fn default() -> Self {
        Self::NotConnected
    }
}

open_string_enum! {
    /// How a provider can be connected (Q998 "browser then key").
    ///
    /// - `browser`: Instabox's loopback + PKCE sign-in, after which Ghostex mints a scoped `ibk_`
    ///   key; Boat's agent-registration claim page with a 6-digit code (needs the user's email).
    /// - `key`: paste an API key (`ibk_…` or `boat_…`).
    /// - `none`: nothing to connect (`local`).
    CloudConnectMethod {
        Browser => "browser",
        Key => "key",
        NotNeeded => "none",
    }
}

open_string_enum! {
    /// A provider snapshot's readiness.
    CloudSnapshotStatus {
        Pending => "pending",
        Ready => "ready",
        Failed => "failed",
    }
}

impl Default for CloudSnapshotStatus {
    fn default() -> Self {
        Self::Pending
    }
}

open_string_enum! {
    /// One line of the boot progress card (Q1009), in the order they run.
    CloudBoxBootStep {
        /// The monthly cap and the provider's plan limits allow a new box.
        CheckCap => "checkCap",
        /// The provider makes the machine from the snapshot (or resumes it).
        Create => "create",
        /// `ghostex box boot`: identity reset, allow-list, gxserver and tailcat up.
        Boot => "boot",
        /// Logins and ticked secrets written into the box's tmpfs folder.
        Secrets => "secrets",
        /// `ghostex box prepare-session`: fetch, branch, start the agent with the first message.
        PrepareSession => "prepareSession",
        /// Clients attach over tailcat and the chat switches to the box session.
        Connect => "connect",
    }
}

impl CloudBoxBootStep {
    /// Every step, in run order.
    pub const ALL: [CloudBoxBootStep; 6] = [
        Self::CheckCap,
        Self::Create,
        Self::Boot,
        Self::Secrets,
        Self::PrepareSession,
        Self::Connect,
    ];

    /// The card's line text.
    pub fn label(&self) -> &str {
        match self {
            Self::CheckCap => "Checking your spending cap",
            Self::Create => "Starting the box",
            Self::Boot => "Starting Ghostex on the box",
            Self::Secrets => "Sending logins and secrets",
            Self::PrepareSession => "Fetching the code and starting the agent",
            Self::Connect => "Connecting",
            Self::Other(value) => value.as_str(),
        }
    }
}

open_string_enum! {
    /// The state of one progress line.
    CloudBoxStepState {
        Waiting => "waiting",
        Running => "running",
        Done => "done",
        Failed => "failed",
        Skipped => "skipped",
    }
}

impl Default for CloudBoxStepState {
    fn default() -> Self {
        Self::Waiting
    }
}

open_string_enum! {
    /// Where this month's spend stands against the cap (D8: warn at 80%, refuse new boxes at
    /// 100%; running boxes always keep running).
    CloudSpendState {
        /// No cap set.
        Off => "off",
        Ok => "ok",
        Warn => "warn",
        Reached => "reached",
    }
}

impl Default for CloudSpendState {
    fn default() -> Self {
        Self::Off
    }
}

open_string_enum! {
    /// What the new-chat tray remembers per project (Q1010; the tray's third segment, Q1027).
    CloudBoxRemember {
        Local => "local",
        Cloud => "cloud",
    }
}

open_string_enum! {
    /// Who made a box: the computer's gxserver or a phone (with the computer off, D7). The phone
    /// form carries its paired device id: `phone:<deviceId>`.
    CloudBoxCreatedBy {
        Computer => "computer",
        Cli => "cli",
    }
}

impl CloudBoxCreatedBy {
    pub const PHONE_PREFIX: &'static str = "phone:";

    pub fn phone(device_id: &str) -> Self {
        Self::Other(format!("{}{device_id}", Self::PHONE_PREFIX))
    }

    /// The paired device id when a phone made the box.
    pub fn phone_device_id(&self) -> Option<&str> {
        match self {
            Self::Other(value) => value.strip_prefix(Self::PHONE_PREFIX),
            _ => None,
        }
    }
}

open_string_enum! {
    /// Which spend figure a provider line is (R6).
    CloudSpendScope {
        /// The provider's own month total for the whole account, including use outside Ghostex
        /// (Instabox `GET /v1/usage`).
        Account => "account",
        /// The sum of Ghostex-made boxes' costs (Boat, which has no stable month total).
        GhostexBoxes => "ghostexBoxes",
    }
}
