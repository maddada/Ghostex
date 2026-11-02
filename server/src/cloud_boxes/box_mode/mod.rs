//! Box mode: what gxserver and the `ghostex box …` verbs do on a box (contract in
//! `ghostex_gx_protocol::cloud_box::box_side`). `boot` here is the minimum the `local` provider
//! needs (start gxserver, and the relay that stands in for tailcat); identity reset, the allow-list
//! and tailcat, the private folder, prepare-session, snapshots, idle and write-back are W2's.

pub(crate) mod boot;
pub(crate) mod relay;

use crate::domain::DomainStateError;

/// The `box*` actions are answered only by a gxserver running on a box.
pub(crate) fn not_in_box_mode() -> DomainStateError {
    DomainStateError {
        code: "unsupported",
        message: "This gxserver is not running on a Cloud Box.".to_string(),
    }
}
