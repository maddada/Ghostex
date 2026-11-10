use crate::rows::{row, section, Page, Section};

pub(crate) fn page() -> Page {
    Page {
        id: "remote",
        title: "Remote",
        sections: vec![
            current_machine(),
            easy_connect(),
            tailscale(),
            remote_machines(),
            remote_advanced(),
        ],
    }
}

pub(crate) fn current_machine() -> Section {
    section(
        "currentMachine",
        "This computer",
        vec![
            row("computerName", "Name", "This computer's name, set by the system; your phone and other computers show it."),
            row("computerShortName", "Short name", "This computer's 2-character name on the sidebar's computer pills; empty uses one made from its name."),
            row("sshAccess", "SSH access", "Lets your phone and other computers sign in to this machine; Easy Connect and Tailscale need it. Ghostex can turn it on with one admin prompt."),
        ],
    )
}

pub(crate) fn easy_connect() -> Section {
    section(
        "easyConnect",
        "Phones and computers that can reach this machine",
        vec![
            row("easyConnectEnabled", "Easy Connect", "Reach this machine from anywhere; only the devices in the list get in. Connect a Device turns it on by itself."),
            row("pairingCode", "Connect a Device", "Opens the Connect a phone window with the code to scan; Connect a Computer shows the code another computer pastes."),
            row("pairedDevices", "Paired devices", "Phones and computers that can reach this machine, and any waiting for Approve; remove one to cut it off."),
        ],
    )
}

pub(crate) fn tailscale() -> Section {
    section(
        "tailscale",
        "Tailscale",
        vec![
            row("tailscaleEnabled", "Tailscale on or off", "Offer the Tailscale path; off leaves it out of the Connect a phone window."),
            row("tailscaleSteps", "Tailscale checklist", "Reach this computer over your tailnet: Tailscale running, SSH access on, the app on your phone."),
            row("tailscaleCode", "Tailscale code", "Scan the Tailscale code with the Ghostex app, or type the host, IP and username."),
        ],
    )
}

pub(crate) fn remote_machines() -> Section {
    section(
        "remoteMachines",
        "Computers this machine connects to",
        vec![
            row("addMachine", "Add a computer", "Add a computer by SSH details or its code; saved computers appear as sidebar sections."),
            row("showInSidebar", "Show in sidebar", "Hide a saved remote machine from the sidebar without deleting it."),
            row("shortName", "Short names", "Each remote machine's 2-character name on the sidebar's computer pills, set in its settings."),
            row("sshHost", "SSH host", "Remote machine SSH host."),
            row("sshUser", "SSH user", "Remote machine SSH user."),
            row("sshPort", "SSH port", "Remote machine SSH port."),
            row("identityFile", "Identity file", "SSH identity file used to connect to the remote machine."),
            row("reuseSshConnection", "Reuse an existing SSH connection", "Use the SSH connection you already signed in to, for computers that ask for a verification code each time."),
            row("password", "Password", "SSH passwords are stored in the system keychain."),
            row("installGxserver", "Install / Connect gxserver", "Install, update, or connect gxserver on a saved remote machine."),
        ],
    )
}

pub(crate) fn remote_advanced() -> Section {
    section(
        "remoteAdvanced",
        "Advanced",
        vec![
            row("easyConnectRelay", "Relay", "Where phones meet this computer: Ghostex's relay first with Tailscale's public relays as fallback, public relays only, or a relay you run."),
            row(
                "servedPorts",
                "Easy Connect served ports",
                "Local ports Easy Connect exposes to paired phones.",
            ),
            row(
                "allowedClientKeys",
                "Allowed client keys",
                "Client keys allowed to connect; empty allows any device that scanned the code.",
            ),
            row(
                "pairingAddress",
                "Pairing address",
                "The raw Easy Connect address inside the QR, for pasting by hand.",
            ),
            row(
                "binary",
                "Easy Connect binary",
                "Path and version of the Easy Connect binary.",
            ),
            row(
                "gxserver",
                "gxserver",
                "Local API the app and phones talk to.",
            ),
            row(
                "rawStatus",
                "Raw Easy Connect status",
                "Raw Easy Connect status JSON for bug reports.",
            ),
        ],
    )
}
