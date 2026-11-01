//! The tables the Settings rows, the native Settings window and the Help files read, one file per area.
//! Names are unique across the files, so the rows import them all through this barrel.

mod agent_accept_all;
mod agent_accounts;
mod agent_skills;
mod completion_sound;
mod defaults;
mod diagnostic_logging;
mod ghostty_config;
mod limits;
mod official_extensions;
mod open_targets;
mod options;
mod presets;
mod project_views;
mod sections;
mod session_card_hover_actions;
mod session_tags;
mod sidebar_agents;
mod sidebar_commands;
mod terminal_fonts;
mod theme_presets;

pub use agent_accept_all::*;
pub use agent_accounts::*;
pub use agent_skills::*;
pub use completion_sound::*;
pub use defaults::*;
pub use diagnostic_logging::*;
pub use ghostty_config::*;
pub use limits::*;
pub use official_extensions::*;
pub use open_targets::*;
pub use options::*;
pub use presets::*;
pub use project_views::*;
pub use sections::*;
pub use session_card_hover_actions::*;
pub use session_tags::*;
pub use sidebar_agents::*;
pub use sidebar_commands::*;
pub use terminal_fonts::*;
pub use theme_presets::*;
