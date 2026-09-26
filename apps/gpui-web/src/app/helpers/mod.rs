//! Same module paths as the desktop's `helpers/`. `chrome_palette` and `indicator_animation` are the desktop files; the others hold only the items `build.rs` lifts out of the desktop files of the same name (see `extracted-items.txt`), because the rest of those files is native-only.
pub(crate) mod chrome_palette;
pub(crate) mod indicator_animation;
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
pub(crate) mod browser {
    use crate::*;
    use gpui::ImageFormat;
    include!(concat!(env!("OUT_DIR"), "/browser.rs"));
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

pub(crate) use agents_hub::*;
pub(crate) use browser::*;
pub(crate) use ids::*;
pub(crate) use project::*;
pub(crate) use web::*;
pub(crate) use chrome_palette::*;
pub(crate) use indicator_animation::*;
pub(crate) use sidebar::*;
pub(crate) use titlebar::*;
