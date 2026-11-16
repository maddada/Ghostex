//! Same module paths as the desktop's `helpers/`. `chrome_palette`, `clipboard_read`, `drag_ghost`, `indicator_animation` and `markdown_table_csv` are the desktop files; the others hold only the items `build.rs` lifts out of the desktop files of the same name (see `extracted-items.txt`), because the rest of those files is native-only.
pub(crate) mod chrome_palette;
pub(crate) mod clipboard_image;
/// The web reads through GPUI Kit fields only, so `gpui_read_clipboard` has no caller here.
#[allow(dead_code)]
pub(crate) mod clipboard_read;
/// The shared drag ghost look; the web draws only some of the ghosts that use it.
#[allow(dead_code)]
pub(crate) mod drag_ghost;
pub(crate) mod indicator_animation;
pub(crate) mod markdown_table_csv;
pub(crate) mod theme_roles;
#[allow(dead_code, unused_imports)]
pub(crate) mod titlebar {
    use crate::app::helpers::*;
    use crate::*;
    include!(concat!(env!("OUT_DIR"), "/titlebar.rs"));
}
#[allow(dead_code, unused_imports)]
pub(crate) mod sidebar {
    use crate::*;
    include!(concat!(env!("OUT_DIR"), "/sidebar.rs"));
}

#[allow(dead_code, unused_imports)]
pub(crate) mod agents_hub {
    use crate::*;
    include!(concat!(env!("OUT_DIR"), "/agents_hub.rs"));
}
#[allow(dead_code, unused_imports)]
pub(crate) mod project {
    use crate::*;
    include!(concat!(env!("OUT_DIR"), "/project.rs"));
}
/// The id checks the desktop's modal and sidebar bridges run on a session or project id, lifted from the desktop files that hold them.
#[allow(dead_code, unused_imports)]
pub(crate) mod ids {
    use crate::*;
    include!(concat!(env!("OUT_DIR"), "/ids.rs"));
}
pub(crate) mod web;

/// The data URL decoders the desktop's `helpers/browser.rs` re-exports (data_url_image.rs).
#[allow(unused_imports)]
pub(crate) use crate::data_url_image::{
    browser_favicon_data_url_metadata, browser_favicon_decode_base64,
    browser_favicon_image_format_for_mime, browser_favicon_percent_decode,
};
pub(crate) use agents_hub::*;
pub(crate) use chrome_palette::*;
pub(crate) use theme_roles::*;
pub(crate) use clipboard_read::*;
pub(crate) use drag_ghost::*;
pub(crate) use ids::*;
pub(crate) use indicator_animation::*;
pub(crate) use project::*;
pub(crate) use sidebar::*;
pub(crate) use titlebar::*;
pub(crate) use web::*;
