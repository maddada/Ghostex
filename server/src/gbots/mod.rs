//! GBots: Ghostex's own bots, led by Chief of Staff in Main Hub. Native GBots are built here
//! (the `gbots_*` tables, the bot folders, `/api/gbots` and `ghostex gbots`); until they land,
//! `/api/gbots` and `ghostex gbots` answer that there are no GBots yet.
//!
//! CDXC:GBots 2026-11-08 DECISION:
//! User: remove the previous GBots fully, "we are doing our own bots thing." The previous GBots, a downloaded runtime gxserver ran beside itself, is gone with its runtime download, transcript mirror, chat agent, Bots space source and settings; its bots and chats are not imported. Native GBots keep the name, the extension switch (`gbotsEnabled`), the `ghostex gbots` command and the `/api/gbots` route. Existing computers lose the old bots once, in retire.rs.

mod endpoint;
mod retire;

pub(crate) use endpoint::gbots_action;
pub(crate) use retire::retire_previous_gbots;
