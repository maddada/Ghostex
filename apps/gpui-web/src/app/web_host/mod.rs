//! The browser's answers to the app-level services the desktop's shared executor files call: toasts, app modals, and the workspace a created or focused session opens in. The desktop performs these with native child windows, CEF pages and its pane workspace; here they are the page's own canvas. Per-concern files; this barrel stays thin.
mod app_modal_commands;
mod create;
mod delayed_send;
mod git_menu;
mod host_messages;
mod modals;
mod project_path_actions;
mod toasts;
mod workspace;

pub(crate) use toasts::WebToasts;

/// What this page holds for those services.
#[derive(Default)]
pub(crate) struct WebHostState {
    pub(crate) toasts: WebToasts,
    /// The page's one window, for work that needs it after an await (`defer_in_main_window`).
    pub(crate) main_window: Option<gpui::AnyWindowHandle>,
    /// Where the header's Commit button was last painted, for the Git menu to hang from.
    pub(crate) git_button_bounds: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    /// The project the Git state was last read for, so a session switch inside it reads nothing.
    pub(crate) git_active_project: Option<String>,
    /// Whether the header's Git dropdown is open (`git_menu.rs`).
    pub(crate) git_menu_open: bool,
}
