//! Chat visual pages: self-contained HTML pages an agent publishes with `ghostex show`, kept under
//! the gxserver state directory and served at `/visual/<id>` so a chat card can open them in a
//! browser page. `prepare` readies the page on the CLI side (local images, theme, CSP, the question
//! kit), `publish` is the authenticated route that stores it, `serve` answers the page itself.

mod answers;
mod builtin;
mod drafts;
mod file_page;
mod fit;
mod images;
mod kit;
mod notice;
mod prepare;
mod publish;
mod questions;
mod scroll;
mod serve;
mod source;
mod store;
mod theme;
mod thumbnail;
mod version;

pub(crate) use answers::{is_answers_route, submit as submit_answers};
pub(crate) use drafts::{is_draft_route, read as read_draft, save as save_draft};
pub(crate) use file_page::page_for_file;
pub(crate) use images::{is_image_route, save as save_page_image};
pub(crate) use notice::{dismiss as dismiss_page_notice, is_notice_route};
pub(crate) use prepare::{page_too_large, prepare_page};
pub(crate) use publish::{
    handle_publish_visual_page_http, PUBLISH_VISUAL_PAGE_BODY_LIMIT_BYTES,
    PUBLISH_VISUAL_PAGE_ENDPOINT,
};
pub(crate) use questions::{page_answered, page_questions};
pub(crate) use scroll::{handle as page_scroll, is_scroll_route};
pub(crate) use serve::{serve, PAGE_SCRIPT_QUERY, VISUAL_PAGE_ROUTE_PREFIX};
pub(crate) use source::{
    handle_read_http as handle_read_visual_page_source_http,
    handle_refresh_http as handle_refresh_visual_page_http, READ_VISUAL_PAGE_SOURCE_ENDPOINT,
    REFRESH_VISUAL_PAGE_ENDPOINT,
};
pub(crate) use store::{prune_old_pages, VISUAL_PAGE_MAX_BYTES};
pub(crate) use thumbnail::{
    handle_read_http as handle_read_visual_page_thumbnail_http,
    handle_store_http as handle_store_visual_page_thumbnail_http,
    READ_VISUAL_PAGE_THUMBNAIL_ENDPOINT, STORE_VISUAL_PAGE_THUMBNAIL_ENDPOINT,
};
pub(crate) use version::{is_current_layout, PAGE_VERSION_META_TAG};
