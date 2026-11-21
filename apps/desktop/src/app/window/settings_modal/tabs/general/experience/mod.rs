//! Settings > Experience (`page.rs`), its preview of the app (`preview.rs`) and what it is trying
//! before Apply (`draft.rs`).
mod draft;
mod page;
mod preview;

pub(crate) use draft::ExperienceDraft;
pub(super) use page::sections;
