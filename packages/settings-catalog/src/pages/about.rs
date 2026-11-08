use crate::rows::{row, section, Page, Section};

pub(crate) fn page() -> Page {
    Page {
        id: "about",
        title: "About",
        sections: vec![about()],
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

// The Experimental section (Faster rendering) moved to Settings > Extensions > Experimental
// (CDXC:Extensions 2026-11-09 in data/official_extensions.rs).
