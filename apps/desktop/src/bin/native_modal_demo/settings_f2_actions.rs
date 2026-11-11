//! Actions and Projects page preview: the Storybook sidebar store's default actions
//! (`createDefaultSidebarCommandButtons`), no Global Actions, and the Projects story's four
//! projects (`storyProjects` of packages/core-ui/settings-modal.stories.tsx (deleted 2026-10-01)).
//!
//! States: `actions` (Settings > Actions, the story's unconfigured defaults), `actions-empty` (no
//! actions: the note and both empty states), `actions-configured` (a Global Action and a
//! configured Dev action with links), `actions-editor-new-terminal`, `actions-editor-new-browser`,
//! `actions-editor-edit` (Dev), `actions-editor-links` (the configured Dev with its links),
//! `actions-duplicate` (Build renamed to Dev), `actions-icon-picker`, `actions-icon-search`;
//! `projects` (the Projects story), `projects-picker`, `projects-picker-search`, `projects-empty`,
//! `projects-inherited` (Global Defaults set, so the project fields inherit), `projects-views`
//! (a Linear website view, a Storybook dev-server view and a selected-only report view); with the
//! Global settings set: `projects-fold-closed` (the fold's summary), `projects-fold-open`,
//! `projects-dropdown` (the Project dropdown open, a worktree under its project),
//! `projects-picked` (Customer Web picked, its fields inheriting) and `projects-search` (a search
//! for "docs directory" matching a global row and a project row). The page reads the dialog and
//! popover states from `preview_state`.
use serde_json::{Map, Value, json};
use std::cell::RefCell;

thread_local! {
    static STATE: RefCell<String> = const { RefCell::new(String::new()) };
}

fn state() -> String {
    STATE.with(|state| state.borrow().clone())
}

pub(super) fn story_settings(state: &str, settings: &mut Map<String, Value>) {
    STATE.with(|current| *current.borrow_mut() = state.to_string());
    if state == "projects-inherited" {
        settings.insert(
            "globalBeadsDirectory".into(),
            json!("/Users/you/shared/.beads"),
        );
        settings.insert(
            "globalDocsDirectory".into(),
            json!("/Users/you/Documents/vault"),
        );
    }
    if matches!(
        state,
        "projects-fold-closed"
            | "projects-fold-open"
            | "projects-dropdown"
            | "projects-picked"
            | "projects-search"
    ) {
        settings.insert("defaultProjectsFolder".into(), json!("~/dev"));
        settings.insert("globalWorktreeCommand".into(), json!("bun install"));
        settings.insert("globalBeadsDisplayKey".into(), json!("TKT"));
        settings.insert("globalDocsDirectory".into(), json!("~/Documents/notes"));
    }
    if state == "projects-views" {
        settings.insert(
            "customViews".into(),
            json!([
                {
                    "availability": "matching",
                    "enabled": true,
                    "id": "custom-linear",
                    "name": "Linear",
                    "projectBindings": { "project-ghostex": { "url": "https://linear.app/ghostex/project/app" } },
                    "source": { "command": "", "cwd": ".", "destination": "project", "discovery": "command", "entry": "index.html", "kind": "website", "readinessUrl": "", "reportDirectory": "", "timeoutSeconds": 60 },
                    "url": ""
                },
                {
                    "availability": "matching",
                    "enabled": true,
                    "id": "custom-storybook",
                    "name": "Components",
                    "source": { "command": "", "cwd": ".", "destination": "fixed", "discovery": "storybook", "entry": "index.html", "kind": "dev-server", "readinessUrl": "", "reportDirectory": "", "timeoutSeconds": 60 },
                    "url": ""
                },
                {
                    "availability": "selected",
                    "enabled": true,
                    "id": "custom-report",
                    "name": "Coverage",
                    "projectIds": [],
                    "source": { "command": "bun run coverage", "cwd": "packages/app", "destination": "fixed", "discovery": "command", "entry": "index.html", "kind": "report", "readinessUrl": "", "reportDirectory": "coverage", "timeoutSeconds": 60 },
                    "url": ""
                }
            ]),
        );
    }
}

/// A project icon as the sidebar gets it: a data URL (a filled rounded square with a bolt).
const GHOSTEX_ICON: &str = "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAzMiAzMiI+PHJlY3Qgd2lkdGg9IjMyIiBoZWlnaHQ9IjMyIiByeD0iOCIgZmlsbD0iI2Y1YTUyNCIvPjxwYXRoIGQ9Ik0xOCA1TDggMThoN2wtMSA5IDEwLTEzaC03bDEtOXoiIGZpbGw9IiMxYTFhMWEiLz48L3N2Zz4=";

/// A worktree's icon: a PNG data URL (an orange square on green).
const WORKTREE_ICON: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAYAAABzenr0AAAAP0lEQVR4nO3OIQEAIBDAwA9DBHqSgiRkQkMBBAbenJiZuqijrcwCAOA0Zy/PAgAAAAAAAAAAAAC4AvwMIB2wAYABiE8U+B1tAAAAAElFTkSuQmCC";

fn default_commands() -> Value {
    json!([
        { "actionType": "terminal", "closeTerminalOnExit": false, "commandId": "dev", "isDefault": true, "name": "Dev", "playCompletionSound": true, "showOnProjectRow": false },
        { "actionType": "terminal", "closeTerminalOnExit": false, "commandId": "build", "isDefault": true, "name": "Build", "playCompletionSound": true, "showOnProjectRow": false },
        { "actionType": "terminal", "closeTerminalOnExit": false, "commandId": "test", "isDefault": true, "name": "Test", "playCompletionSound": true, "showOnProjectRow": false },
        { "actionType": "terminal", "closeTerminalOnExit": false, "commandId": "setup", "isDefault": true, "name": "Setup", "playCompletionSound": true, "showOnProjectRow": false }
    ])
}

fn configured_commands() -> Value {
    json!([
        {
            "actionType": "terminal", "closeTerminalOnExit": false, "command": "bun run dev", "commandId": "dev", "icon": "rocket", "isDefault": true,
            "links": [
                { "target": "integrated", "url": "http://localhost:5173" },
                { "target": "external", "url": "http://localhost:6006" }
            ],
            "name": "Dev", "playCompletionSound": true, "showOnProjectRow": true
        },
        { "actionType": "terminal", "closeTerminalOnExit": true, "command": "bun run build\nbun run check", "commandId": "build", "icon": "package", "isDefault": true, "name": "Build", "playCompletionSound": true, "showOnProjectRow": false },
        { "actionType": "browser", "closeTerminalOnExit": false, "commandId": "custom-docs", "icon": "fileText", "isDefault": false, "name": "Docs", "playCompletionSound": false, "showOnProjectRow": false, "url": "http://localhost:3000/docs" }
    ])
}

pub(super) fn extend_sidebar_state(message: &mut Value) {
    let state = state();
    let configured = matches!(
        state.as_str(),
        "actions-configured" | "actions-editor-links"
    );
    message["hud"]["commands"] = if state == "actions-empty" {
        json!([])
    } else if configured {
        configured_commands()
    } else {
        default_commands()
    };
    message["hud"]["globalCommands"] = if configured {
        json!([
            { "actionType": "browser", "closeTerminalOnExit": false, "commandId": "global-status", "icon": "world", "isDefault": false, "name": "Status page", "playCompletionSound": false, "showOnProjectRow": false, "url": "https://status.example.com" }
        ])
    } else {
        json!([])
    };
    message["hud"]["projectSettingsProjects"] = if state == "projects-empty" {
        json!([])
    } else {
        json!([
            { "beadsDirectory": "", "beadsDisplayKey": "ZMX", "iconDataUrl": GHOSTEX_ICON, "name": "Ghostex", "path": "/Users/you/dev/ghostex", "projectId": "project-ghostex", "worktreeCommand": "bun install", "workMode": true, "workspaceId": "work" },
            { "beadsDirectory": "/Users/you/dev/infra/.beads", "beadsDisplayKey": "INF", "name": "Infra Control Plane", "path": "/Users/you/dev/platform/infra-control-plane", "projectId": "project-infra", "worktreeCommand": "pnpm install" },
            { "beadsDirectory": "", "beadsDisplayKey": "WEB", "name": "Customer Web", "path": "/Users/you/dev/products/customer-web-application", "projectId": "project-web", "worktreeCommand": "" },
            { "beadsDirectory": "", "beadsDisplayKey": "OPS", "name": "Operations Dashboard", "path": "/Users/you/dev/internal/tools/operations-dashboard", "projectId": "project-ops", "worktreeCommand": "bun run setup" },
            { "beadsDirectory": "", "beadsDisplayKey": "", "iconDataUrl": WORKTREE_ICON, "name": "ghostex-settings-fold", "path": "/Users/you/dev/ghostex-settings-fold", "projectId": "project-ghostex-fold", "worktreeCommand": "", "worktreeParentProjectId": "project-ghostex" }
        ])
    };
}

pub(super) fn open_message(state: &str) -> Option<Value> {
    if state == "actions" || state.starts_with("actions-") {
        return Some(json!({ "initialTab": "actions" }));
    }
    if state == "projects-search" {
        // GHOSTEX_NATIVE_MODAL_DEMO_QUERY searches for something else.
        let query = std::env::var("GHOSTEX_NATIVE_MODAL_DEMO_QUERY")
            .ok()
            .filter(|query| !query.trim().is_empty())
            .unwrap_or_else(|| "docs directory".to_string());
        return Some(json!({ "initialTab": "projects", "initialSearchQuery": query }));
    }
    if state == "projects" || state.starts_with("projects-") {
        return Some(json!({ "initialTab": "projects" }));
    }
    None
}
