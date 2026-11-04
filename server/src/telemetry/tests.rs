//! Unit tests for the extension whitelist, the `surface.opened` and heartbeat shapes, and the
//! opt-out. Nothing here touches the process-global telemetry handle or the network: the opt-out
//! test builds a private `Telemetry` whose queue is inspected and never flushed.

use std::time::{Duration, SystemTime};

use serde_json::{json, Value};

use super::{
    capture::surface_opened_properties,
    client_events::surface_opened_details,
    extensions::{self, EnabledExtensions, ExtensionRef},
    heartbeat::{heartbeat_properties, HeartbeatSnapshot},
    taxonomy::{self, PropertyValue, EVENT_HEARTBEAT, EVENT_SURFACE_OPENED},
};

/// An id published by the official Store for these tests. The Store set is add-only and shared
/// by every test in this binary, so each test uses ids no other test treats as custom.
const OFFICIAL_ID: &str = "test-official-canvas";
const CUSTOM_ID: &str = "my-secret-tool";

fn remember_official() {
    extensions::remember_official_store_ids([OFFICIAL_ID]);
}

fn snapshot(enabled_extensions: EnabledExtensions) -> HeartbeatSnapshot {
    HeartbeatSnapshot {
        project_count: 3,
        session_count: 10,
        running_session_count: 2,
        agents_used: vec!["claude", "codex"],
        custom_agent_executables: vec![],
        default_agent: "claude",
        preferred_interface: "chat",
        extension_count: 3,
        built_in_extensions_on: extensions::built_in_extensions_on(Some(&json!({
            "githubViewTabHidden": false,
        }))),
        enabled_extensions,
        remote_machine_count: 0,
        days_since_install: 4,
    }
}

#[test]
fn built_in_ids_resolve_to_the_catalog() {
    assert_eq!(
        extensions::resolve_extension("github"),
        ExtensionRef::BuiltIn("github")
    );
    assert_eq!(
        extensions::resolve_extension("storybook"),
        ExtensionRef::BuiltIn("storybook")
    );
    // The desktop's Claude view id is aliased to its catalog entry.
    assert_eq!(
        extensions::resolve_extension("claude-web"),
        ExtensionRef::BuiltIn("claudeWeb")
    );
}

#[test]
fn official_store_id_is_kept_and_custom_id_is_not() {
    remember_official();
    assert_eq!(
        extensions::resolve_extension(OFFICIAL_ID),
        ExtensionRef::Store(OFFICIAL_ID.to_string())
    );
    assert_eq!(
        extensions::resolve_extension(CUSTOM_ID),
        ExtensionRef::Custom
    );
    assert_eq!(extensions::resolve_extension(""), ExtensionRef::Custom);
}

#[test]
fn store_catalog_cache_feeds_the_whitelist() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("catalog-cache.json"),
        json!({
            "schemaVersion": 1,
            "extensions": [
                { "name": "test-cached-btop" },
                { "name": "Bad Name With Spaces" },
            ],
        })
        .to_string(),
    )
    .unwrap();
    extensions::load_official_store_ids(dir.path());
    assert!(extensions::is_official_store_id("test-cached-btop"));
    assert!(!extensions::is_official_store_id("Bad Name With Spaces"));
}

#[test]
fn enabled_report_lists_official_ids_and_counts_the_rest() {
    remember_official();
    let report = extensions::enabled_extensions_report([
        (OFFICIAL_ID, true),
        (CUSTOM_ID, true),
        ("another-local-thing", true),
        ("test-disabled-custom", false),
    ]);
    assert_eq!(report.store, vec![OFFICIAL_ID.to_string()]);
    assert_eq!(report.custom_count, 2);
}

#[test]
fn surface_opened_for_an_official_extension_view() {
    remember_official();
    let (extension, client) = surface_opened_details(
        "extension",
        Some(&json!({ "surface": "extension", "extension": OFFICIAL_ID, "client": "desktop" })),
    )
    .unwrap();
    let properties = surface_opened_properties("extension", extension.as_ref(), client);
    let encoded = taxonomy::validate(EVENT_SURFACE_OPENED, &properties).unwrap();
    assert_eq!(
        Value::Object(encoded),
        json!({ "surface": "extension", "extension": OFFICIAL_ID, "client": "desktop" })
    );
}

#[test]
fn surface_opened_never_carries_a_custom_extension_id() {
    let (extension, client) =
        surface_opened_details("extension", Some(&json!({ "extension": CUSTOM_ID }))).unwrap();
    let properties = surface_opened_properties("extension", extension.as_ref(), client);
    let encoded = Value::Object(taxonomy::validate(EVENT_SURFACE_OPENED, &properties).unwrap());
    assert_eq!(
        encoded,
        json!({ "surface": "extension", "extension": "custom" })
    );
    assert!(!encoded.to_string().contains(CUSTOM_ID));

    // A caller that smuggles a custom id in as a Store id has the whole event dropped.
    let smuggled = surface_opened_properties(
        "extension",
        Some(&ExtensionRef::Store(CUSTOM_ID.to_string())),
        None,
    );
    assert!(taxonomy::validate(EVENT_SURFACE_OPENED, &smuggled).is_err());
}

#[test]
fn surface_opened_shapes_for_views_and_phone_screens() {
    for surface in [
        "changes",
        "terminal",
        "work",
        "claude",
        "automations",
        "quick_access",
    ] {
        let surface = taxonomy::match_enum(taxonomy::SURFACES, surface).unwrap();
        let (extension, client) = surface_opened_details(surface, None).unwrap();
        assert!(extension.is_none());
        let encoded = taxonomy::validate(
            EVENT_SURFACE_OPENED,
            &surface_opened_properties(surface, None, client),
        )
        .unwrap();
        assert_eq!(Value::Object(encoded), json!({ "surface": surface }));
    }

    let (_, client) =
        surface_opened_details("mobile_terminal", Some(&json!({ "client": "mobile" }))).unwrap();
    assert_eq!(client, Some("mobile"));
    // A phone screen must come from the phone, and a desktop view must not.
    assert!(surface_opened_details("mobile_terminal", None).is_none());
    assert!(surface_opened_details("changes", Some(&json!({ "client": "mobile" }))).is_none());
    assert!(surface_opened_details("changes", Some(&json!({ "client": "watch" }))).is_none());
    assert!(taxonomy::match_enum(taxonomy::SURFACES, "my-secret-view").is_none());
}

#[test]
fn heartbeat_carries_the_extension_snapshot() {
    remember_official();
    let report = extensions::enabled_extensions_report([(OFFICIAL_ID, true), (CUSTOM_ID, true)]);
    let encoded = taxonomy::validate(EVENT_HEARTBEAT, &heartbeat_properties(&snapshot(report)))
        .map(Value::Object)
        .unwrap();
    assert_eq!(encoded["store_extensions_enabled"], json!([OFFICIAL_ID]));
    assert_eq!(encoded["custom_extensions_enabled"], json!(1.0));
    let built_in = encoded["built_in_extensions_on"].as_array().unwrap();
    assert!(built_in.contains(&json!("github")));
    // Off by default and not switched on in the settings above.
    assert!(!built_in.contains(&json!("claudeWeb")));
    assert!(!encoded.to_string().contains(CUSTOM_ID));
}

#[test]
fn heartbeat_with_a_smuggled_store_id_is_dropped() {
    let mut snapshot = snapshot(EnabledExtensions::default());
    snapshot.enabled_extensions.store = vec![CUSTOM_ID.to_string()];
    assert!(taxonomy::validate(EVENT_HEARTBEAT, &heartbeat_properties(&snapshot)).is_err());

    // A Store id inside the built-in list is rejected too.
    let mixed = [(
        "built_in_extensions_on",
        PropertyValue::ExtensionList(vec![ExtensionRef::Custom]),
    )];
    assert!(taxonomy::validate(EVENT_HEARTBEAT, &mixed).is_err());
}

fn write_settings(paths: &crate::paths::GxserverPaths, settings: Value, modified: SystemTime) {
    std::fs::create_dir_all(&paths.app_config_dir).unwrap();
    let path = paths.app_config_dir.join(super::gate::SETTINGS_FILE_NAME);
    std::fs::write(&path, settings.to_string()).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(modified)
        .unwrap();
}

#[test]
fn opt_out_queues_nothing_and_opt_in_queues_the_event() {
    remember_official();
    let home = tempfile::tempdir().unwrap();
    let paths = crate::paths::get_gxserver_paths(Some(home.path().to_path_buf()));
    let telemetry = super::queue::telemetry_for_test(paths.clone());
    let properties = surface_opened_properties(
        "extension",
        Some(&ExtensionRef::Store(OFFICIAL_ID.to_string())),
        Some("desktop"),
    );

    // Distinct mtimes, because the gate's settings cache is keyed by mtime.
    let base = SystemTime::now() - Duration::from_secs(600);
    write_settings(&paths, json!({ "analyticsEnabled": false }), base);
    telemetry.enqueue(EVENT_SURFACE_OPENED, &properties);
    assert!(telemetry.queued_for_test().is_empty());

    write_settings(
        &paths,
        json!({ "analyticsEnabled": true }),
        base + Duration::from_secs(60),
    );
    telemetry.enqueue(EVENT_SURFACE_OPENED, &properties);
    let queued = telemetry.queued_for_test();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].0, EVENT_SURFACE_OPENED);
    assert_eq!(queued[0].1["extension"], json!(OFFICIAL_ID));

    // Opting out again drops what was queued, so it is never sent.
    write_settings(
        &paths,
        json!({ "analyticsEnabled": false }),
        base + Duration::from_secs(120),
    );
    telemetry.enqueue(EVENT_SURFACE_OPENED, &properties);
    assert!(telemetry.queued_for_test().is_empty());
}
