//! The General pages' search rows (layout.rs), one function per section.
mod browser;
mod chat;
mod experience;
mod sidebar;
mod system;
mod terminal;
mod theme;
mod tools;

use crate::rows::Section;
use crate::Platform;

pub fn sections(platform: Platform) -> Vec<Section> {
    vec![
        experience::experience(),
        tools::file_opening(),
        tools::grok_web(),
        browser::agents(),
        browser::links(),
        browser::pages(),
        browser::web_runtime(),
        browser::sign_ins(),
        tools::editor(),
        system::computer_startup(),
        system::memory_guard(),
        system::auto_sleep(),
        system::power(),
        sidebar::session_cards(),
        sidebar::sidebar(),
        sidebar::spaces(),
        theme::theming(),
        theme::text_fonts(),
        theme::size_zoom(platform),
        theme::window_layout(),
        chat::chat(),
        chat::marked_handovers(),
        sidebar::sidebar_tags(),
        chat::sounds(),
        terminal::terminal(platform),
        terminal::terminal_behavior(platform),
        terminal::terminal_scrolling(),
        tools::terminal_dev_servers(),
        system::sleeping_sessions(),
    ]
}
