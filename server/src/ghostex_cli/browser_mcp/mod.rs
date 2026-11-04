//! `ghostex browser` and its stdio DevTools MCP server, split by concern from the former
//! `ghostex_cli/browser_mcp.rs`. Every submodule is glob re-exported here, so callers keep
//! using `browser_mcp::…` unchanged. The module's unit tests stay inline at the end of
//! `page_scripts.rs`.

/*
CDXC:Cli 2026-07-13:
Faithful port of the Node CLI's `browser` namespace: subcommand dispatch,
browser usage text, the openBrowserPane bridge payload, and the full stdio MCP
server (JSON-RPC over stdio; framing shared with the connectors server in mcp_stdio.rs) that drives Ghostex's embedded CEF
pages over the Chrome DevTools Protocol. The page-side scripts (snapshot,
click, fill) are the verbatim JS function sources the Node CLI produced via
`fn.toString()`; they still execute inside the page's JS engine through
Runtime.evaluate, so they must not be "translated" to Rust.
*/

mod agent_pages;
mod cdp_client;
mod cdp_pages;
mod command;
mod js_values;
mod mcp_server;
mod mcp_tools;
mod page_scripts;

use agent_pages::*;
use cdp_client::*;
use cdp_pages::*;
pub use command::*;
use js_values::*;
use mcp_server::*;
use mcp_tools::*;
use page_scripts::*;
