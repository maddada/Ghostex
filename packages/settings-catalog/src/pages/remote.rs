use crate::rows::{row, section, Page, Section};

pub(crate) fn page() -> Page {
    Page {
        id: "remote",
        title: "Remote",
        sections: vec![
            easy_connect(),
            tailscale(),
            remote_machines(),
            remote_advanced(),
        ],
    }
}

pub(crate) fn easy_connect() -> Section {
    section(
        "easyConnect",
        "Easy Connect",
        vec![
            row("easyConnectEnabled", "Easy Connect", "Connect a phone or a remote machine. Install the Tailcat CLI helper with one click."),
            row("sshAccess", "SSH access", "Easy Connect carries SSH to this computer; Ghostex can turn it on with one admin prompt."),
            row("pairingCode", "Pairing code", "Connect a Phone with a QR, or Connect a Remote machine with Copy Easy Connect code and its SSH username and password."),
            row("pairedDevices", "Paired devices", "Phones and computers paired with this computer; remove one to unpair it and revoke its access."),
            row("pairedDevicesOnly", "Only paired devices can connect", "Easy Connect lets in only paired phones and the keys listed under Advanced; a new phone can still pair while the QR is showing."),
            row("easyConnectRelay", "Relay", "Where phones meet this computer: Ghostex's relay first with Tailscale's public relays as fallback, public relays only, or a relay you run."),
        ],
    )
}

pub(crate) fn tailscale() -> Section {
    section(
        "tailscale",
        "Tailscale",
        vec![
            row("tailscaleEnabled", "Tailscale on or off", "Offer the Tailscale path; off keeps its card collapsed and hides it from Remote Setup."),
            row("tailscaleSteps", "Tailscale checklist", "Reach this computer over your tailnet: Tailscale running, SSH access on, the app on your phone."),
            row("tailscaleCode", "Tailscale code", "Scan the Tailscale code with the Ghostex app, or type the host, IP and username."),
        ],
    )
}

pub(crate) fn remote_machines() -> Section {
    section(
        "remoteMachines",
        "Remote machines",
        vec![
            row("addMachine", "Add a machine", "Add a computer by SSH details or an Easy Connect code; saved machines appear as sidebar sections."),
            row("showInSidebar", "Show in sidebar", "Hide a saved remote machine from the sidebar without deleting it."),
            row("shortName", "Short names", "Each computer's 2-character name on the sidebar's computer pills, this one's included."),
            row("sshHost", "SSH host", "Remote machine SSH host."),
            row("sshUser", "SSH user", "Remote machine SSH user."),
            row("sshPort", "SSH port", "Remote machine SSH port."),
            row("identityFile", "Identity file", "SSH identity file used to connect to the remote machine."),
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
