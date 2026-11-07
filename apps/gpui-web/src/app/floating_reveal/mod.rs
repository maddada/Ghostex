pub(crate) mod window;
#[allow(dead_code)]
pub(crate) mod model {
    include!(concat!(env!("OUT_DIR"), "/floating_reveal_model.rs"));
}

impl crate::GhostexGpuiApp {
    /// The page never floats the sidebar in a window of its own (window.rs), so the links card's
    /// hover gate (native_sidebar/session_hover.rs) always sees a docked sidebar.
    pub(crate) fn floating_sidebar_shown(&self) -> bool {
        false
    }

    /// A browser page has no traffic lights, so no row keeps room for them.
    pub(crate) fn window_controls_reserved(&self) -> bool {
        false
    }

    /// A browser page has no traffic lights to show over the header.
    pub(crate) fn window_lights_over_header(&self) -> bool {
        false
    }

    /// The page never floats the sidebar, so its top row never shows the lights.
    pub(crate) fn floating_sidebar_lights_row(&self) -> Option<(f32, f32)> {
        None
    }
}
