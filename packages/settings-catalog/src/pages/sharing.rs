use crate::rows::{row, section, Page, Section};

/// Settings > Sharing (under Connections): built-in tunnel sharing for Live pages and localhost
/// dev ports (`/api/sharing`, `server/src/sharing/`).
pub(crate) fn page() -> Page {
    Page {
        id: "sharing",
        title: "Sharing",
        sections: vec![switch(), provider(), defaults(), active()],
    }
}

fn switch() -> Section {
    section(
        "switch",
        "Sharing",
        vec![row(
            "sharingEnabled",
            "Enable sharing",
            "Share individual Live pages or local dev server ports over temporary encrypted links. Turning this off stops every active share immediately.",
        )],
    )
}

fn provider() -> Section {
    section(
        "provider",
        "Tunnel provider",
        vec![
            row(
                "sharingProvider",
                "Provider",
                "OpenTunnel by default: blind end-to-end TLS where the relay never reads your HTML, headers or cookies, with no account required.",
            ),
            row(
                "sharingRelayUrl",
                "Relay URL",
                "Optional self-hosted OpenTunnel API address (OPENTUNNEL_API). Leave blank to use the default hosted relay (https://opentunnel.xyz).",
            ),
        ],
    )
}

fn defaults() -> Section {
    section(
        "defaults",
        "New shares start with",
        vec![
            row(
                "sharingDefaultExpiry",
                "Default link expiry",
                "How long a new share link stays live: 1 hour, 24 hours (the default), 7 days, or Until stopped. Links stop immediately when stopped or when Ghostex quits.",
            ),
            row(
                "sharingDefaultPasscode",
                "Require passcode by default",
                "Protect newly shared Live pages and local ports with a 6-digit passcode by default. Off by default; you can turn a passcode on per share.",
            ),
        ],
    )
}

fn active() -> Section {
    section(
        "active",
        "Active shares",
        vec![row(
            "sharingActiveShares",
            "Active share links",
            "Every Live page and local port currently shared from this computer, with Copy link, Stop, and Stop all.",
        )],
    )
}
