#![cfg(unix)]

use std::{fs, os::unix::fs::symlink, path::Path};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use gxserver::project_docs::{run_project_docs_action, ProjectDocsExtraRoot, ProjectDocsRoot};
use serde_json::{json, Map, Value};

/// The reserved first path segment that addresses the mounted Docs directory.
/// Mirrors `EXTRA_ROOT_MOUNT_SEGMENT` in `server/src/project_docs.rs`.
const MOUNT: &str = ".ghostex-docs-root";

fn params(value: Value) -> Map<String, Value> {
    value.as_object().expect("object params").clone()
}

/// A project that configures no Docs directory: only the project root is
/// mounted and discovery stays exactly as it has always been.
fn project_root(path: &Path) -> ProjectDocsRoot {
    ProjectDocsRoot {
        project_path: path.to_path_buf(),
        extra: None,
    }
}

/// A project whose Docs directory is mounted in addition to its own root.
fn with_docs_directory(project: &Path, docs_directory: &Path) -> ProjectDocsRoot {
    ProjectDocsRoot {
        project_path: project.to_path_buf(),
        extra: Some(ProjectDocsExtraRoot {
            path: docs_directory.to_path_buf(),
            error: None,
        }),
    }
}

/// A project whose configured Docs directory could not be opened.
fn with_missing_docs_directory(project: &Path, configured: &Path) -> ProjectDocsRoot {
    ProjectDocsRoot {
        project_path: project.to_path_buf(),
        extra: Some(ProjectDocsExtraRoot {
            path: configured.to_path_buf(),
            error: Some(format!(
                "Docs directory does not exist: {}",
                configured.display()
            )),
        }),
    }
}

fn mounted(relative_path: &str) -> String {
    if relative_path.is_empty() {
        MOUNT.to_string()
    } else {
        format!("{MOUNT}/{relative_path}")
    }
}

fn write_file(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    fs::write(path, contents).expect("write file");
}

fn list(root: &ProjectDocsRoot, additional_docs_folders: &str) -> Value {
    run_project_docs_action(
        root,
        &params(json!({
            "action": "list",
            "additionalDocsFolders": additional_docs_folders,
            "projectId": "Premote",
            "requestId": "list-1",
        })),
    )
}

fn read(root: &ProjectDocsRoot, additional_docs_folders: &str, path: &str) -> Value {
    run_project_docs_action(
        root,
        &params(json!({
            "action": "read",
            "additionalDocsFolders": additional_docs_folders,
            "path": path,
            "projectId": "Premote",
            "requestId": "read-1",
        })),
    )
}

fn entry_paths(response: &Value) -> Vec<String> {
    assert_eq!(response.get("error"), None, "unexpected Docs error");
    response["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|entry| entry["path"].as_str().expect("entry path").to_string())
        .collect()
}

fn entry_depth(response: &Value, path: &str) -> u64 {
    entry(response, path)["depth"]
        .as_u64()
        .expect("entry depth")
}

fn entry<'a>(response: &'a Value, path: &str) -> &'a Value {
    response["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|entry| entry["path"] == path)
        .unwrap_or_else(|| panic!("{path} is not listed"))
}

/// A notes vault: nested folders, files Docs does not render, and the folders
/// that must never be walked.
fn vault_fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("vault tempdir");
    let root = temp.path();
    write_file(&root.join("root.md"), "# root");
    write_file(&root.join("image.png"), "not a document");
    write_file(&root.join("Personal/journal.md"), "# journal");
    write_file(&root.join("notes/top.md"), "# top");
    write_file(&root.join("notes/deep/deeper/note.md"), "# deep note");
    write_file(&root.join(".obsidian/workspace.json"), "{}");
    write_file(&root.join(".git/config"), "[core]");
    write_file(&root.join("node_modules/pkg/readme.md"), "# vendored");
    temp
}

/// A repository: the docs/ folder, a root artifact, and code Docs has never
/// listed.
fn repo_fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("repo tempdir");
    let root = temp.path();
    write_file(&root.join("README.md"), "# readme");
    write_file(&root.join("notes.txt"), "plain");
    write_file(&root.join("docs/guide.md"), "# guide");
    write_file(&root.join("docs/sub/deep.md"), "# deep");
    write_file(&root.join("src/main.rs"), "fn main() {}");
    write_file(&root.join("node_modules/pkg/index.md"), "# vendored");
    temp
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
        &project_root(project.path()),
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
        &project_root(project.path()),
        &params(json!({
            "action": "read",
            "path": "docs/guide.md",
            "projectId": "Premote",
            "requestId": "read-1",
        })),
    );
    assert_eq!(read["file"]["content"], "# Remote guide\n");

    let saved = run_project_docs_action(
        &project_root(project.path()),
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
        &project_root(project.path()),
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
        &project_root(project.path()),
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
        &project_root(project.path()),
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
        &project_root(project.path()),
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

/// A Docs directory ADDS a tree; it never takes the project's own docs away.
#[test]
fn a_docs_directory_keeps_the_projects_own_entries() {
    let repo = repo_fixture();
    let vault = vault_fixture();
    let listed = list(&with_docs_directory(repo.path(), vault.path()), "");
    let paths = entry_paths(&listed);

    for own in ["docs", "README.md", "docs/guide.md", "docs/sub/deep.md"] {
        assert!(
            paths.contains(&own.to_string()),
            "the project's own {own} stopped listing: {paths:?}"
        );
    }
    assert_eq!(
        read(
            &with_docs_directory(repo.path(), vault.path()),
            "",
            "README.md",
        )["file"]["content"],
        "# readme"
    );
}

#[test]
fn a_docs_directory_mounts_its_whole_tree_under_one_top_level_node() {
    let repo = repo_fixture();
    let vault = vault_fixture();
    let root = with_docs_directory(repo.path(), vault.path());
    let listed = list(&root, "");
    let paths = entry_paths(&listed);

    // One top-level node, named after the folder itself.
    assert_eq!(entry_depth(&listed, MOUNT), 0);
    assert_eq!(
        entry(&listed, MOUNT)["name"],
        Value::String(
            vault
                .path()
                .file_name()
                .expect("vault name")
                .to_string_lossy()
                .into_owned()
        )
    );
    assert_eq!(entry(&listed, MOUNT)["kind"], "directory");

    // Nested three deep inside it, listed and readable.
    let nested = mounted("notes/deep/deeper/note.md");
    assert!(paths.contains(&nested), "nested note is missing: {paths:?}");
    assert_eq!(entry_depth(&listed, &nested), 4);
    assert!(paths.contains(&mounted("Personal/journal.md")));
    assert!(paths.contains(&mounted("root.md")));
    let response = read(&root, "", &nested);
    assert_eq!(response.get("error"), None);
    assert_eq!(response["file"]["content"], "# deep note");
    assert_eq!(response["file"]["path"], nested);

    // Only what the Docs surface renders, and never the folders round 2 excluded.
    assert!(!paths.contains(&mounted("image.png")));
    for excluded in [".obsidian", ".git", "node_modules"] {
        assert!(
            !paths
                .iter()
                .any(|path| path.starts_with(&mounted(excluded))),
            "{excluded} was walked: {paths:?}"
        );
    }
}

/// Every mounted entry names itself by the mount, not by the routing segment,
/// so Copy Path and pasted feedback never leak `.ghostex-docs-root`.
#[test]
fn mounted_entries_carry_the_mount_name_as_their_display_path() {
    let repo = repo_fixture();
    let vault = vault_fixture();
    let root = with_docs_directory(repo.path(), vault.path());
    let listed = list(&root, "");
    let vault_name = vault
        .path()
        .file_name()
        .expect("vault name")
        .to_string_lossy()
        .into_owned();

    // The mount node itself, and a note nested three folders inside it.
    assert_eq!(
        entry(&listed, MOUNT)["displayPath"],
        Value::String(vault_name.clone())
    );
    assert_eq!(
        entry(&listed, &mounted("notes/deep/deeper/note.md"))["displayPath"],
        Value::String(format!("{vault_name}/notes/deep/deeper/note.md"))
    );

    // The project's own files route by the name they already show.
    assert_eq!(entry(&listed, "README.md").get("displayPath"), None);

    // The routing address is untouched: it is what a request must send back.
    assert_eq!(
        entry(&listed, &mounted("root.md"))["path"],
        Value::String(mounted("root.md"))
    );
}

/// Docs folders stays project-root-relative; it does not narrow the mount.
#[test]
fn docs_folders_narrow_the_project_root_only() {
    let repo = repo_fixture();
    let vault = vault_fixture();
    let paths = entry_paths(&list(
        &with_docs_directory(repo.path(), vault.path()),
        "src",
    ));

    assert!(paths.contains(&"src".to_string()));
    assert!(paths.contains(&"src/main.rs".to_string()));
    assert!(paths.contains(&"docs/guide.md".to_string()));
    assert!(paths.contains(&mounted("notes/top.md")));
}

#[test]
fn a_path_addressed_to_one_root_cannot_reach_the_other_or_escape() {
    let repo = repo_fixture();
    let vault = vault_fixture();
    let elsewhere = tempfile::tempdir().expect("outside tempdir");
    let outside_note = elsewhere.path().join("outside-note.md");
    fs::write(&outside_note, "# outside").expect("outside note");
    symlink(&outside_note, vault.path().join("escape.md")).expect("symlink");
    fs::write(repo.path().join("secret.txt"), "not docs").expect("secret file");

    let root = with_docs_directory(repo.path(), vault.path());
    let paths = entry_paths(&list(&root, ""));
    assert!(
        !paths.contains(&mounted("escape.md")),
        "an outward symlink joined the tree: {paths:?}"
    );

    for escaping in [
        // Out of the mount, in every direction.
        mounted("../outside-note.md"),
        mounted("escape.md"),
        mounted("notes/../../outside-note.md"),
        // Out of the mount and into the project root.
        mounted("../README.md"),
        format!("{MOUNT}/../{MOUNT}/root.md"),
        // Out of the project root and into the mount.
        "../root.md".to_string(),
        "docs/../secret.txt".to_string(),
    ] {
        let response = read(&root, "", &escaping);
        assert!(
            response["error"].as_str().is_some(),
            "{escaping} was readable"
        );
    }

    // The decisive case: a name that exists in BOTH roots resolves to the root
    // it was addressed to, never the other.
    write_file(&vault.path().join("README.md"), "# vault readme");
    assert_eq!(read(&root, "", "README.md")["file"]["content"], "# readme");
    assert_eq!(
        read(&root, "", &mounted("README.md"))["file"]["content"],
        "# vault readme"
    );
    // And the project root never answers for a file only the mount has.
    assert!(read(&root, "", "root.md")["error"].as_str().is_some());
}

/// Writes answer with the address the Docs page can use again.
#[test]
fn writes_inside_the_mount_answer_with_the_mounted_path() {
    let repo = repo_fixture();
    let vault = vault_fixture();
    let root = with_docs_directory(repo.path(), vault.path());
    let note = mounted("notes/top.md");

    let saved = run_project_docs_action(
        &root,
        &params(json!({
            "action": "save",
            "content": "# edited in the vault\n",
            "path": note,
            "projectId": "Premote",
            "requestId": "save-mounted",
        })),
    );
    assert_eq!(saved.get("error"), None);
    assert_eq!(saved["file"]["path"], note);
    assert_eq!(saved["file"]["content"], "# edited in the vault\n");
    assert_eq!(
        fs::read_to_string(vault.path().join("notes/top.md")).expect("saved note"),
        "# edited in the vault\n"
    );

    // A rename that would straddle the two roots is refused outright.
    let straddle = run_project_docs_action(
        &root,
        &params(json!({
            "action": "rename",
            "newPath": "docs/top.md",
            "path": note,
            "projectId": "Premote",
            "requestId": "rename-straddle",
        })),
    );
    assert!(straddle["error"]
        .as_str()
        .is_some_and(|error| error.contains("between the project and the Docs directory")));
}

/// The one place a partial result is correct — and it must be labeled.
#[test]
fn a_missing_docs_directory_still_lists_the_projects_own_docs() {
    let repo = repo_fixture();
    let missing = repo.path().join("no-such-vault");
    let listed = list(&with_missing_docs_directory(repo.path(), &missing), "");
    let paths = entry_paths(&listed);

    assert!(paths.contains(&"README.md".to_string()));
    assert!(paths.contains(&"docs/guide.md".to_string()));
    let label = entry(&listed, MOUNT)["name"]
        .as_str()
        .expect("mount label")
        .to_string();
    assert!(
        label.contains("no-such-vault") && label.contains("does not exist"),
        "the unavailable Docs directory was not labeled: {label}"
    );
}

#[test]
fn project_root_discovery_is_unchanged() {
    let repo = repo_fixture();
    let paths = entry_paths(&list(&project_root(repo.path()), ""));

    assert_eq!(
        paths,
        vec![
            "docs".to_string(),
            "README.md".to_string(),
            "docs/sub".to_string(),
            "docs/guide.md".to_string(),
            "docs/sub/deep.md".to_string(),
        ]
    );

    let source = read(&project_root(repo.path()), "", "src/main.rs");
    assert!(source["error"].as_str().is_some());
}
