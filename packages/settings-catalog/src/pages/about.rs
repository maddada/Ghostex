use crate::rows::{row, section, Page, Section};

pub(crate) fn page() -> Page {
    Page {
        id: "about",
        title: "About",
        sections: vec![about(), beta()],
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

/// CDXC:Settings 2026-10-29 DECISION: User (Q5): with the Advanced group gone, Experimental lives on the About page, after the links. Its rows stay advanced (`ADVANCED_MAIN_SETTING_KEYS`), so About folds them into its Advanced section, or shows them in place for a matching search.
pub(crate) fn beta() -> Section {
    section(
        "beta",
        "Experimental",
        vec![
            // Enable Experimental Features is gone: each thing it turned on is its own switch in
            // Settings > Extensions > Experimental (CDXC:Extensions 2026-11-09 in
            // data/official_extensions.rs).
            row(
                "fasterRendering",
                "Faster rendering",
                "Redraw only the parts of a window that changed, which uses less CPU while agents stream and you scroll. Experimental: turn it off if part of a window stops updating.",
            ),
        ],
    )
}
