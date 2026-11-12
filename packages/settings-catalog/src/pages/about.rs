use crate::rows::{row, section, Page, Section};

pub(crate) fn page() -> Page {
    Page {
        id: "about",
        title: "About",
        sections: vec![about(), privacy()],
    }
}

pub(crate) fn about() -> Section {
    section(
        "about",
        "About",
        vec![
            row("version", "Version", "Ghostex app version."),
            row(
                "discord",
                "Join Discord",
                "Chat with the community and get help.",
            ),
            row(
                "github",
                "View on GitHub",
                "View the source, releases, and report issues.",
            ),
            row(
                "sponsor",
                "Sponsor Ghostex",
                "Support the continued development of Ghostex.",
            ),
        ],
    )
}

/// The usage-data opt-out that setup points to (CDXC:Onboarding 2026-11-11 in
/// apps/desktop/src/app/window/onboarding/get_started.rs). gxserver reads the key before every
/// event it sends (server/src/telemetry/gate.rs).
pub(crate) fn privacy() -> Section {
    section(
        "privacy",
        "Privacy",
        vec![row(
            "analyticsEnabled",
            "Anonymous usage analytics",
            "Share usage data (your OS, which features you use, counts) to help improve Ghostex. Never your prompts, file paths, project names or email.",
        )],
    )
}

// The Experimental section (Faster rendering) moved to Settings > Extensions > Experimental
// (CDXC:Extensions 2026-11-09 in data/official_extensions.rs).
