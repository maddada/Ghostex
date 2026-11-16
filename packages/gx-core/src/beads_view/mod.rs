//! The Beads view's rules (the Tree layout, who is on a bead, the date words), shared by the
//! desktop and the GPUI web build. gxserver supplies bd's facts (`/api/beadsView`); this module
//! only arranges them.

mod agents;
pub mod json_api;
mod time;
mod tree;

pub use agents::{
    bead_agent_key, bead_agents, BeadAgent, BeadAgentSession, BeadAgentState, BeadAgents,
};
pub use time::{
    local_date_after, parse_bd_time_ms, relative_age, relative_ago, short_date, BeadsClock,
};
pub use tree::{
    bead_details, build_tree, matches_filter, status_label, type_label, BeadDetailsModel,
    BeadGlyph, BeadMeta, BeadRowModel, BeadsFilter, BeadsFilterCounts, BeadsSection,
    BeadsSectionKind, BeadsSort, BeadsTreeModel, BeadsTreeOptions, EpicSummary,
};
