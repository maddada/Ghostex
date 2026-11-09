//! "Keep an eye on my day": a GBot, Day Watcher, that reads the user's Gmail and Google Calendar
//! through Connectors and briefs them in Chief of Staff's chat.
//!
//! - [`ops`]: `/api/gbots` `dayWatch`, `dayWatchOn`, `dayWatchOff` (`ghostex gbots day-watch …`).
//! - [`deliver`]: a run's result to Chief of Staff, and the alert on its reply.
//! - [`offer`]: Chief of Staff offers it once, when Gmail or Google Calendar is connected.
//! - [`store`]: the `gbots_day_watch` row; [`text`]: its persona, prompts and hand-off words.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User (plan piece 8, Q-E A): the day watch is a GBot with two routines, a daily brief (08:00 by default, with the fixed sections Needs you, Your day, Important emails, Routines) and a watch every 15 minutes whose cheap checks are new important mail and a meeting about to start; it is set up only when the user asks ("keep an eye on my day") or accepts it from Chief of Staff, and the brief lands in Chief of Staff's chat with a notification and a phone push. It never writes to the user's apps (apps_access.rs).
//! CDXC:GBots 2026-11-09 WHY:
//! `ghostex gbots day-watch on` sets it up; Chief of Staff runs it when the user asks or accepts its one offer, so Day Watcher is a GBot Chief of Staff added, with the "GBot added" card and Undo, rather than a part of Chief of Staff (its routines, pause and removal then work like any GBot's). The user turns it off by telling Chief of Staff (`day-watch off` pauses both routines and keeps the GBot), pausing a routine or the GBot in GBot Settings, or removing the GBot. The watch runs from 07:00 to 21:45 so a quiet night costs nothing, and in one standing chat so it remembers what it already told.

mod deliver;
mod offer;
mod ops;
mod store;
mod text;

pub(crate) use deliver::{brief_turn_alerts, deliver};
pub(crate) use offer::start_day_watch_offer;
pub(crate) use ops::{adds_bot, handles, run};
