//! Attention on the desktop: `report.rs` tells a session's daemon what the user did (web-ready:
//! `gx_rpc` and gx-core only), `host.rs` feeds the store's attention intents and performs what they
//! ask for (the acknowledgement timer, the report, the completion sound). The rules are gx-core's
//! `attention.rs`.

mod host;
mod report;
