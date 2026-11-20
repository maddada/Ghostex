//! Prints the one session menu (src/sidebar_menu/layout.rs) as the JSON the phone reads, for the
//! phone's bundled copy (apps/mobile/app/scripts/generate-session-menu-layout.mjs), which its menus
//! use when the chat core cannot answer.
fn main() {
    let layout = ghostex_gx_core::session_menu_layout::layout_json();
    println!(
        "{}",
        serde_json::to_string_pretty(&layout).expect("the session menu layout is JSON")
    );
}
