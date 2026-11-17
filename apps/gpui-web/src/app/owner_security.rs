//! The web build's stand-in for the desktop's `owner_security` (native key storage and Touch ID):
//! a page holds no owner key of its own, so no approval request is ever this app's own ask.

use ghostex_gx_protocol::approval::ApprovalRequest;

pub(crate) fn asks_for_this_app(_request: &ApprovalRequest) -> bool {
    false
}

pub(crate) fn reported_computer_name(_request: &ApprovalRequest) -> Option<String> {
    None
}
