//! Settings > Updates (`/api/updates`): everything this computer's gxserver can update, in one list
//! with one Update all, a per-item "Update automatically" switch, and the count the chat's top bar
//! shows.
//!
//! CDXC:Updates 2026-11-05 DECISION:
//! User: "we need a page to update everything related to ghostex ... for all the used/enabled features that need updates in a grid and also to update the agents. And it should have 1 update all button and we should show the updates somehow in the top bar of the chat". Picks on the design page: Q1360 the page is a Settings tab, Q1361 a quiet badge in the top bar with Update all one click away, Q1362 the badge counts the Ghostex app, agents in use, failures, and (user's note) cua-driver, SpaceO, Codex Swap and Claude Swap when installed, plus the VS Code view when enabled and CEF; Q1363 running agents restart into the same chat once idle; Q1364 Update all includes the Ghostex app, last; Q1365 a "Update automatically" switch on every card, off by default, that installs only while that thing isn't in use.
//! WHY: every update goes through the path its own Settings row already uses (agent_cli's jobs, accounts' helper actions, managed_tools' jobs, the extension registry, app_update's install and its "Keep this machine up to date" switch), so this module only lists, orders and remembers; it never installs anything itself. The desktop adds what only it can see (remote machines, the VS Code view, the Chromium runtime) in apps/desktop/src/app/updates/.
//! SEE-ALSO: apps/desktop/src/app/window/settings_modal/tabs/updates/ (the page), apps/desktop/src/app/titlebar/updates_menu.rs (the top bar badge).
mod auto;
mod http;
mod install;
pub(crate) mod items;
mod missing;
mod store;

pub(crate) use auto::spawn_updates_auto_task;
pub(crate) use http::handle_updates_http;
