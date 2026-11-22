use serde_json::{json, Value};

use super::*;

#[test]
fn title_projection_strips_factory_droid_status_marker_from_stored_title() {
    let mut session = session("P100", "G100", "\u{26ec} New Session", "running", 1000.0);
    let session_object = session.as_object_mut().expect("session object");
    session_object.insert("agentId".to_string(), json!("droid"));
    session_object.insert(
        "runtimeSettings".to_string(),
        json!({
            "agentName": "factory droid",
            "titleSource": "terminal-auto"
        }),
    );

    let projection = project_session_title_projection(&session);

    assert_eq!(
        projection.get("displayTitle").and_then(Value::as_str),
        Some("New Session")
    );
    assert_eq!(
        projection
            .get("displayTitleTooltip")
            .and_then(Value::as_str),
        Some("New Session")
    );
    assert_eq!(
        projection.get("primaryTitle").and_then(Value::as_str),
        Some("New Session")
    );
    assert_eq!(
        projection.get("trustedResumeTitle").and_then(Value::as_str),
        Some("New Session")
    );
    assert_eq!(
        projection.get("title").and_then(Value::as_str),
        Some("\u{26ec} New Session")
    );
}

#[test]
fn title_projection_strips_omp_idle_and_spinner_prefixes() {
    for raw_title in [
        "  \u{03c0} >   Delete marketplace skill and inventory skills  ",
        "  \u{03c0} \u{2827}   Delete marketplace skill and inventory skills  ",
    ] {
        let mut session = session("P100", "G100", raw_title, "running", 1000.0);
        let session_object = session.as_object_mut().expect("session object");
        session_object.insert("agentId".to_string(), json!("omp"));
        session_object.insert(
            "runtimeSettings".to_string(),
            json!({ "agentName": "omp", "titleSource": "terminal-auto" }),
        );

        let projection = project_session_title_projection(&session);

        assert_eq!(
            projection.get("displayTitle").and_then(Value::as_str),
            Some("Delete marketplace skill and inventory skills")
        );
        assert_eq!(
            projection
                .get("displayTitleTooltip")
                .and_then(Value::as_str),
            Some("Delete marketplace skill and inventory skills")
        );
        assert_eq!(
            projection.get("primaryTitle").and_then(Value::as_str),
            Some("Delete marketplace skill and inventory skills")
        );
    }
}

#[test]
fn title_projection_replaces_wsl_shell_location_with_terminal_default_title() {
    let mut session = session(
        "P100",
        "G100",
        "madda@M7-Desktop: /mnt/c/dev/Ghostex",
        "running",
        1000.0,
    );
    let session_object = session.as_object_mut().expect("session object");
    session_object.insert(
        "runtimeSettings".to_string(),
        json!({ "titleSource": "terminal-auto" }),
    );

    let projection = project_session_title_projection(&session);

    assert_eq!(
        projection.get("primaryTitle").and_then(Value::as_str),
        Some("Terminal Session")
    );
    assert_eq!(projection.get("trustedResumeTitle"), None);
    assert_eq!(
        projection.get("displayTitle").and_then(Value::as_str),
        Some("\u{2217} Terminal Session")
    );
}
