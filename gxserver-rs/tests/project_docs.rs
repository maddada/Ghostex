use std::{fs, path::Path};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use gxserver::project_docs::run_project_docs_action;
use serde_json::{json, Map, Value};

fn params(value: Value) -> Map<String, Value> {
    value.as_object().expect("object params").clone()
}

#[test]
fn lists_reads_saves_and_serves_registered_project_docs() {
    let project = tempfile::tempdir().expect("project tempdir");
    fs::create_dir(project.path().join("docs")).expect("docs directory");
    fs::write(project.path().join("docs/guide.md"), "# Remote guide\n").expect("guide file");
    fs::write(
        project.path().join("docs/theme.css"),
        "body { color: red; }",
    )
    .expect("resource file");

    let listed = run_project_docs_action(
        project.path(),
        &params(json!({
            "action": "list",
            "projectId": "Premote",
            "requestId": "list-1",
        })),
    );
    assert_eq!(listed["requestId"], "list-1");
    assert!(listed["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .any(|entry| entry["path"] == "docs/guide.md"));

    let read = run_project_docs_action(
        project.path(),
        &params(json!({
            "action": "read",
            "path": "docs/guide.md",
            "projectId": "Premote",
            "requestId": "read-1",
        })),
    );
    assert_eq!(read["file"]["content"], "# Remote guide\n");

    let saved = run_project_docs_action(
        project.path(),
        &params(json!({
            "action": "save",
            "content": "# Updated remotely\n",
            "path": "docs/guide.md",
            "projectId": "Premote",
            "requestId": "save-1",
        })),
    );
    assert_eq!(saved["file"]["content"], "# Updated remotely\n");
    assert_eq!(
        fs::read_to_string(project.path().join("docs/guide.md")).expect("saved guide"),
        "# Updated remotely\n"
    );

    let resource = run_project_docs_action(
        project.path(),
        &params(json!({
            "action": "readResource",
            "path": "docs/theme.css",
            "projectId": "Premote",
            "requestId": "resource-1",
        })),
    );
    let bytes = BASE64_STANDARD
        .decode(resource["dataBase64"].as_str().expect("resource data"))
        .expect("base64 resource");
    assert_eq!(bytes, b"body { color: red; }");
}

#[test]
fn rejects_paths_outside_the_docs_allowlist() {
    let project = tempfile::tempdir().expect("project tempdir");
    fs::create_dir(project.path().join("docs")).expect("docs directory");
    fs::write(project.path().join("secret.txt"), "not docs").expect("secret file");

    let outside = run_project_docs_action(
        project.path(),
        &params(json!({
            "action": "read",
            "path": "secret.txt",
            "projectId": "Premote",
            "requestId": "outside-1",
        })),
    );
    assert!(outside["error"]
        .as_str()
        .is_some_and(|error| error.contains("configured Docs folders")));

    let traversal = run_project_docs_action(
        Path::new(project.path()),
        &params(json!({
            "action": "read",
            "path": "docs/../secret.txt",
            "projectId": "Premote",
            "requestId": "traversal-1",
        })),
    );
    assert!(traversal["error"]
        .as_str()
        .is_some_and(|error| error.contains("inside the project")));
}

#[test]
fn stops_directory_enumeration_at_the_global_scan_limit() {
    let project = tempfile::tempdir().expect("project tempdir");
    let docs = project.path().join("docs");
    fs::create_dir(&docs).expect("docs directory");
    for index in (0..1_300).rev() {
        fs::write(docs.join(format!("guide-{index:04}.md")), index.to_string())
            .expect("docs entry");
    }

    let listed = run_project_docs_action(
        project.path(),
        &params(json!({
            "action": "list",
            "projectId": "Premote",
            "requestId": "bounded-list",
        })),
    );
    assert_eq!(listed["action"], "list");
    assert_eq!(listed["requestId"], "bounded-list");
    assert!(listed["error"]
        .as_str()
        .is_some_and(|error| error.contains("too many directory entries")));
}
