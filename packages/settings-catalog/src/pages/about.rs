use crate::rows::{row, section, Page, Section};

pub(crate) fn page() -> Page {
    Page {
        id: "about",
        title: "About",
        sections: vec![about(), privacy()],
    }
}

pub(crate) fn privacy() -> Section {
    section(
        "privacy",
        "Privacy & Analytics",
        vec![row(
            "analyticsEnabled",
            "Share 100% Fully Anonymous Usage Analytics to Support Ghostex Development",
            "Zero personal data. Helps us continue supporting and building Ghostex. Turning this off stops all usage tracking and keeps only 1 anonymous weekly active count (stop all with DO_NOT_TRACK=1 or --no-analytics).",
        )],
    )
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

// The Experimental section (Faster rendering) moved to Settings > Extensions > Experimental
// (CDXC:Extensions 2026-11-09 in data/official_extensions.rs).
