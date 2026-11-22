//! Chat visual pages: self-contained HTML pages an agent publishes with `ghostex show`, kept under
//! the gxserver state directory and served at `/visual/<id>` so a chat card can open them in a
//! browser page. `prepare` readies the page on the CLI side (local images, theme, CSP, the question
//! kit), `publish` is the authenticated route that stores it, `serve` answers the page itself.

mod answers;
mod builtin;
mod drafts;
mod exported;
mod file_page;
mod fit;
mod guest;
mod images;
mod import;
mod kit;
mod linked;
mod notice;
mod pick_markdown;
mod prepare;
mod publish;
mod questions;
mod retire;
mod scroll;
mod serve;
mod source;
mod store;
mod theme;
mod thumbnail;
mod version;

pub(crate) use answers::{is_answers_route, submit as submit_answers};
pub(crate) use drafts::{is_draft_route, read as read_draft, save as save_draft};
pub(crate) use exported::{
    EXPORT_VISUAL_PAGE_ENDPOINT, download as download_exported_page,
    handle_export_http as handle_export_visual_page_http, is_export_route,
};
pub(crate) use file_page::{linked_page_summary, page_for_file};
pub(crate) use guest::{guest_file_page, guest_page, guest_page_answers_message};
pub(crate) use images::{is_image_route, read as read_page_image, save as save_page_image};
pub(crate) use import::{
    IMPORT_VISUAL_PAGE_ANSWERS_ENDPOINT,
    handle_import_http as handle_import_visual_page_answers_http,
};
pub(crate) use linked::{follow as follow_page_link, is_link_route};
pub(crate) use notice::{dismiss as dismiss_page_notice, is_notice_route};
pub(crate) use prepare::{page_too_large, prepare_page};
pub(crate) use publish::{
    PUBLISH_VISUAL_PAGE_BODY_LIMIT_BYTES, PUBLISH_VISUAL_PAGE_ENDPOINT,
    handle_publish_visual_page_http,
};
pub(crate) use questions::{page_answered, page_questions};
pub(crate) use retire::{
    RETIRE_VISUAL_PAGES_ENDPOINT, handle_retire_visual_pages_http, page_id_of,
    read_retired_page_ids, take_back_retired_pages,
};
pub(crate) use scroll::{handle as page_scroll, is_scroll_route};
pub(crate) use serve::{PAGE_SCRIPT_QUERY, VISUAL_PAGE_ROUTE_PREFIX, serve};
pub(crate) use source::{
    READ_VISUAL_PAGE_SOURCE_ENDPOINT, REFRESH_VISUAL_PAGE_ENDPOINT,
    handle_read_http as handle_read_visual_page_source_http,
    handle_refresh_http as handle_refresh_visual_page_http,
};
pub(crate) use store::{VISUAL_PAGE_MAX_BYTES, page_source_path, prune_old_pages};
pub(crate) use thumbnail::{
    READ_VISUAL_PAGE_THUMBNAIL_ENDPOINT, STORE_VISUAL_PAGE_THUMBNAIL_ENDPOINT,
    handle_read_http as handle_read_visual_page_thumbnail_http,
    handle_store_http as handle_store_visual_page_thumbnail_http,
};
pub(crate) use version::{PAGE_VERSION_META_TAG, is_current_layout};
