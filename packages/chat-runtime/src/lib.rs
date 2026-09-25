//! The desktop's client storage (`client-storage.sqlite3`): its start-up (tables, the browser-era
//! import, the catalog migrations) and the records and preferences doors the Rust store reads and
//! writes. The QuickJS host that also lived here (the chat's runtime, then the app runtime) was
//! deleted on 2026-09-25 with QuickJS itself.

mod storage;
mod storage_catalog;
mod storage_import;
mod storage_import_docs;
mod storage_init;
mod storage_metadata;
mod storage_records;
pub use storage_catalog::{CATALOG, CatalogBackend, CatalogStore, definition_for_key};
pub use storage_import_docs::import_docs_browser_state;
pub use storage_init::{StorageInitReport, initialize_client_storage};
pub use storage_metadata::{
    RecordStoreUsage, apply_record_metadata, recompute_record_metadata, scan_record_usage,
};
pub use storage_records::{
    MAX_BACKEND_BYTES, RecordRead, RecordStore, RecordWrite, read_record, storage_bytes,
    write_record,
};
