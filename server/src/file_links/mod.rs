//! Private browser links to a project's files: the Files view's Open Externally hands one to the
//! default browser, and the phone plays and shows a session's files through one. HTML is served as
//! it is, Markdown as a rendered page, Excalidraw drawings as an editor page that saves back.

mod pages;
mod protected;
mod registry;
mod serve;

pub(crate) use registry::{file_link_path, link_path};
pub(crate) use serve::{serve, FILE_LINK_ROUTE_PREFIX};
