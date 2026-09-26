//! The notification feed's reads and commands: `feed.rs` talks to gxserver (web-ready: `gx_rpc`
//! and gx-core only), `commands.rs` performs what the bell's panel and the jump keys ask for.

mod commands;
mod feed;
