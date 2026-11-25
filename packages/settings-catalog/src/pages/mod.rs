//! The searchable pages other than General, Theme and Hotkeys, in the order Settings lists them.
mod about;
mod accounts;
mod actions;
mod agents;
mod autopilot;
mod cloud_boxes;
mod collab;
mod connectors;
mod debugging;
mod dictation;
mod extensions;
mod grok_bot;
mod integrations;
mod open_targets;
mod os_integration;
mod projects;
mod remote;
mod secrets;
mod security;
mod sharing;
mod sync;
mod updates;
mod voice_agent;
mod workspaces;

use crate::rows::Page;
use crate::Platform;

pub fn pages(platform: Platform) -> Vec<Page> {
    vec![
        updates::page(),
        debugging::page(),
        about::page(),
        actions::page(),
        accounts::page(),
        autopilot::page(),
        collab::page(),
        agents::page(),
        integrations::page(),
        grok_bot::page(),
        cloud_boxes::page(),
        extensions::page(platform),
        open_targets::page(),
        os_integration::page(),
        projects::page(),
        remote::page(),
        sync::page(),
        sharing::page(),
        secrets::page(),
        security::page(),
        workspaces::page(),
        connectors::page(),
        dictation::page(),
        voice_agent::page(),
    ]
}
