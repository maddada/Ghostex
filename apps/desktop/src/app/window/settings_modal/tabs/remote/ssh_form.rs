//! The Add a computer form's shared part: its fields, their limits, their wording, what a field's
//! change does to the draft, and whether the draft can be saved. Settings > Remote's Add a
//! machine / edit dialog (machines.rs) and the Connect a computer window's SSH details tab
//! (connect_device_modal/ssh_tab.rs) both use it, so a computer added in either gets the same
//! checks and the same words; each draws the fields in its own surface.
//!
//! CDXC:RemotePairing 2026-11-16 DECISION:
//! User, of the Connect a computer window's SSH details tab: "Form in the tab". The tab shows the
//! Add a computer form itself (name, SSH host, username, password, and Advanced) instead of a
//! button that opens Settings > Remote, and both surfaces share this module.
use super::model::{MachineDraft, create_remote_machine_draft, normalize_remote_machine_draft};
use gpui::{AppContext as _, Context, Entity, Subscription, Window};
use gpui_component::input::{InputEvent, InputState};
use serde_json::Value;

/// Which draft field an input edits.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DraftField {
    Code,
    Name,
    ShortName,
    Host,
    User,
    Password,
    Port,
    Identity,
    Wsl,
}

impl DraftField {
    /// The input's `maxLength`.
    pub(crate) fn max_length(self) -> usize {
        match self {
            Self::Code => 4000,
            Self::Name => 80,
            Self::ShortName => ghostex_gx_core::machine_short_names::MACHINE_SHORT_NAME_MAX,
            Self::Host => 200,
            Self::User => 120,
            Self::Password => 500,
            Self::Port => 5,
            Self::Identity => 500,
            Self::Wsl => 120,
        }
    }

    /// What the field takes of `text`: digits only for the port, clipped to `maxLength`.
    pub(crate) fn clip(self, text: &str) -> String {
        let text: String = if self == Self::Port {
            text.chars().filter(char::is_ascii_digit).collect()
        } else {
            text.to_string()
        };
        text.chars().take(self.max_length()).collect()
    }
}

/// The wording of the form: placeholders and the explanation each field carries.
pub(crate) mod words {
    pub(crate) const CODE_PLACEHOLDER: &str = "Paste the code copied from the other computer";
    pub(crate) const NAME_PLACEHOLDER: &str = "Machine one";
    pub(crate) const SHORT_NAME_PLACEHOLDER: &str = "WN";
    pub(crate) const HOST_PLACEHOLDER: &str = "100.77.81.4";
    pub(crate) const USER_PLACEHOLDER: &str = "machine username";
    pub(crate) const PORT_PLACEHOLDER: &str = "22";
    pub(crate) const IDENTITY_PLACEHOLDER: &str = "~/.ssh/id_ed25519";
    pub(crate) const WSL_PLACEHOLDER: &str = "Ubuntu-24.04";
    pub(crate) const USER_INFO: &str = "The account you sign in with on the other computer.";
    pub(crate) const IDENTITY_INFO: &str =
        "Optional. Use an SSH private key instead of a password.";
    pub(crate) const WSL_INFO: &str = "Optional. Enter a distribution to use WSL2. Leave blank to follow Windows Environment on the remote computer, which defaults to native PowerShell.";
    pub(crate) const AGENT_ACCESS_TITLE: &str = "Agent access";
    pub(crate) const AGENT_ACCESS_INFO: &str = "Agents on this computer can start, message and read sessions on this machine, and its agents can do the same here.";
    pub(crate) const REUSE_SSH_TITLE: &str = "Reuse an existing SSH connection";
    pub(crate) const REUSE_SSH_INFO: &str = "For computers that ask for a verification code each time you sign in. Sign in once in a terminal with ControlMaster on in your SSH config, and Ghostex uses that connection instead of signing in again.";
    /// The new computer's form says what it takes; Ghostex installs gxserver on it when it
    /// connects and finds it missing.
    pub(crate) const NEW_INTRO: &str =
        "Enter the other computer’s address and the username and password you sign in with there.";

    /// The password field's placeholder.
    pub(crate) fn password_placeholder(saved: bool) -> &'static str {
        if saved {
            "Saved in Keychain"
        } else {
            "SSH password"
        }
    }

    /// The password field's explanation.
    pub(crate) fn password_info(is_new: bool) -> &'static str {
        if is_new {
            "Use the password for this username on the other computer. If you already use SSH keys, leave this blank and set the key under Advanced."
        } else {
            "Passwords are stored in secure system storage. Leave blank and press Save to remove a saved password."
        }
    }
}

/// The form's inputs, one per field.
pub(crate) struct SshFormInputs {
    pub(crate) code: Entity<InputState>,
    pub(crate) name: Entity<InputState>,
    pub(crate) short_name: Entity<InputState>,
    pub(crate) host: Entity<InputState>,
    pub(crate) user: Entity<InputState>,
    pub(crate) password: Entity<InputState>,
    pub(crate) port: Entity<InputState>,
    pub(crate) identity: Entity<InputState>,
    pub(crate) wsl: Entity<InputState>,
}

impl SshFormInputs {
    /// The inputs on `draft`'s values; `on_change` hears each change (the owner clips the text and
    /// follows it in its draft).
    pub(super) fn new<V: 'static>(
        draft: &MachineDraft,
        window: &mut Window,
        cx: &mut Context<V>,
        on_change: impl Fn(&mut V, DraftField, String, Entity<InputState>, &mut Window, &mut Context<V>)
        + Clone
        + 'static,
    ) -> (Self, Vec<Subscription>) {
        let make = |field: DraftField,
                    value: &str,
                    placeholder: &str,
                    window: &mut Window,
                    cx: &mut Context<V>| {
            let value = value.to_string();
            let placeholder = placeholder.to_string();
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .masked(field == DraftField::Password)
                    .default_value(value)
            })
        };
        let inputs = Self {
            code: make(
                DraftField::Code,
                &draft.easy_connect_code,
                words::CODE_PLACEHOLDER,
                window,
                cx,
            ),
            name: make(
                DraftField::Name,
                &draft.name,
                words::NAME_PLACEHOLDER,
                window,
                cx,
            ),
            short_name: make(
                DraftField::ShortName,
                &draft.short_name,
                words::SHORT_NAME_PLACEHOLDER,
                window,
                cx,
            ),
            host: make(
                DraftField::Host,
                &draft.ssh_host,
                words::HOST_PLACEHOLDER,
                window,
                cx,
            ),
            user: make(
                DraftField::User,
                &draft.ssh_user,
                words::USER_PLACEHOLDER,
                window,
                cx,
            ),
            password: make(
                DraftField::Password,
                "",
                words::password_placeholder(draft.ssh_password_saved),
                window,
                cx,
            ),
            port: make(
                DraftField::Port,
                &draft.ssh_port,
                words::PORT_PLACEHOLDER,
                window,
                cx,
            ),
            identity: make(
                DraftField::Identity,
                &draft.ssh_identity_file,
                words::IDENTITY_PLACEHOLDER,
                window,
                cx,
            ),
            wsl: make(
                DraftField::Wsl,
                &draft.wsl_distribution,
                words::WSL_PLACEHOLDER,
                window,
                cx,
            ),
        };
        let mut subscriptions = Vec::new();
        for (field, input) in [
            (DraftField::Code, &inputs.code),
            (DraftField::Name, &inputs.name),
            (DraftField::ShortName, &inputs.short_name),
            (DraftField::Host, &inputs.host),
            (DraftField::User, &inputs.user),
            (DraftField::Password, &inputs.password),
            (DraftField::Port, &inputs.port),
            (DraftField::Identity, &inputs.identity),
            (DraftField::Wsl, &inputs.wsl),
        ] {
            let on_change = on_change.clone();
            subscriptions.push(cx.subscribe_in(
                input,
                window,
                move |owner: &mut V, input, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Change) {
                        let text = input.read(cx).value().to_string();
                        on_change(owner, field, text, input.clone(), window, cx);
                    }
                },
            ));
        }
        (inputs, subscriptions)
    }
}

/// Puts the clipped `text` back into `input` when the field took less of it than was typed.
pub(crate) fn put_back_clipped<V: 'static>(
    field: DraftField,
    text: &str,
    input: &Entity<InputState>,
    window: &mut Window,
    cx: &mut Context<V>,
) -> String {
    let next = field.clip(text);
    if next != text {
        let value = next.clone();
        input.update(cx, |input, cx| input.set_value(value, window, cx));
    }
    next
}

/// A plain text field's value into the draft (the code field has its own reading).
pub(super) fn apply_field(draft: &mut MachineDraft, field: DraftField, next: String) {
    match field {
        DraftField::Code => {}
        DraftField::Name => draft.name = next,
        DraftField::ShortName => draft.short_name = next,
        DraftField::Host => draft.ssh_host = next,
        DraftField::User => draft.ssh_user = next,
        DraftField::Password => draft.ssh_password = next,
        DraftField::Port => draft.ssh_port = next,
        DraftField::Identity => draft.ssh_identity_file = next,
        DraftField::Wsl => draft.wsl_distribution = next,
    }
}

/// Whether `draft` can be saved, and what is missing when it cannot.
pub(super) fn save_check(draft: &MachineDraft) -> (bool, &'static str) {
    let can_save = normalize_remote_machine_draft(draft).is_some();
    let reason = if draft.easy_connect {
        if matches!(
            super::model::read_easy_connect_code_input(&draft.easy_connect_code),
            super::model::CodeReading::Accepted { .. }
        ) || !draft.easy_connect_address.is_empty()
        {
            "Enter a machine name first."
        } else {
            "Paste a valid Easy Connect code first."
        }
    } else if !draft.name.trim().is_empty() && !draft.ssh_host.trim().is_empty() {
        "Check the WSL distribution name."
    } else {
        "Enter a machine name and SSH host first."
    };
    (can_save, reason)
}

/// A new computer's form for a window that hosts no Settings page: the draft, its inputs, and
/// what the tab needs of them.
pub(crate) struct SshComputerForm {
    pub(crate) inputs: SshFormInputs,
    draft: MachineDraft,
    pub(crate) advanced_open: bool,
    _subscriptions: Vec<Subscription>,
}

impl SshComputerForm {
    pub(crate) fn new<V: 'static>(
        window: &mut Window,
        cx: &mut Context<V>,
        on_change: impl Fn(&mut V, DraftField, String, Entity<InputState>, &mut Window, &mut Context<V>)
        + Clone
        + 'static,
    ) -> Self {
        let draft = create_remote_machine_draft();
        let (inputs, subscriptions) = SshFormInputs::new(&draft, window, cx, on_change);
        Self {
            inputs,
            draft,
            advanced_open: false,
            _subscriptions: subscriptions,
        }
    }

    /// One field changed: the draft follows it, clipped to the field's limit.
    pub(crate) fn field_changed<V: 'static>(
        &mut self,
        field: DraftField,
        text: String,
        input: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<V>,
    ) {
        let next = put_back_clipped(field, &text, input, window, cx);
        apply_field(&mut self.draft, field, next);
    }

    pub(crate) fn can_save(&self) -> (bool, &'static str) {
        save_check(&self.draft)
    }

    pub(crate) fn agent_access(&self) -> bool {
        !self.draft.agent_access_off
    }

    pub(crate) fn toggle_agent_access(&mut self) {
        self.draft.agent_access_off = !self.draft.agent_access_off;
    }

    pub(crate) fn reuse_ssh_connection(&self) -> bool {
        self.draft.reuse_ssh_connection
    }

    pub(crate) fn toggle_reuse_ssh_connection(&mut self) {
        self.draft.reuse_ssh_connection = !self.draft.reuse_ssh_connection;
    }

    /// Whether the reuse-a-connection switch is offered: not for a Windows WSL machine.
    pub(crate) fn offers_reuse_ssh_connection(&self) -> bool {
        cfg!(any(target_os = "macos", target_os = "linux"))
            && self.draft.wsl_distribution.trim().is_empty()
    }

    /// The computer's name as typed.
    pub(crate) fn name(&self) -> &str {
        self.draft.name.trim()
    }

    /// The saved-computer entry the draft makes, and the password still to go to the keychain.
    pub(crate) fn computer(&self) -> Option<(Value, String)> {
        Some((
            normalize_remote_machine_draft(&self.draft)?,
            self.draft.ssh_password.clone(),
        ))
    }
}
