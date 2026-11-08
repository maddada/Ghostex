//! Issue #488: Spaces sync on gxserver with a monorepo of several folder projects, on two
//! computers, through the same engine steps an exchange runs (export, merge, import).

use std::path::Path;
use std::sync::Arc;

use ghostex_gx_core::settings_sync::{next_hlc, SyncEntry};
use serde_json::{json, Map, Value};

use super::engine::with_engine;
use super::spaces_bridge::read_local_spaces;
use super::store::{SpacesBaseline, SyncStore};
use crate::domain::DomainRepository;
use crate::project_git_remote::{set_cached_project_git_remote_for_test, ProjectGitRemote};
use crate::server::AppState;
use crate::sidebar_project_collections::read_sidebar_project_collections;
use crate::sidebar_spaces::{read_sidebar_spaces, update_sidebar_spaces};

const ORIGIN: &str = "git@github.com:me/monorepo.git";
const MONOREPO: &str = "github.com/me/monorepo";

fn computer(home: &Path, server_id: &str) -> Arc<AppState> {
    let state = crate::server::tests::helpers::test_app_state_for(
        crate::paths::get_gxserver_paths(Some(home.to_path_buf())),
        server_id,
    );
    std::fs::create_dir_all(&state.paths.app_config_dir).expect("config dir");
    std::fs::write(
        state
            .paths
            .app_config_dir
            .join("native-sidebar-settings.json"),
        json!({ "syncSpaces": true }).to_string(),
    )
    .expect("settings file");
    state
}

fn params(value: Value) -> Map<String, Value> {
    value.as_object().cloned().expect("params object")
}

/// The monorepo's root and the given folders as projects of `state`, each probed as a folder of
/// one repository, returning (folder or "" for the root, project id).
fn monorepo(state: &AppState, home: &Path, folders: &[&str]) -> Vec<(String, String)> {
    let root = home.join("code").join("monorepo");
    let db = crate::storage::open_gxserver_database(&state.paths).expect("open db");
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    folders
        .iter()
        .map(|folder| {
            let path = if folder.is_empty() {
                root.clone()
            } else {
                root.join(folder)
            };
            std::fs::create_dir_all(&path).expect("project folder");
            let path_text = path.to_string_lossy().to_string();
            set_cached_project_git_remote_for_test(
                &path_text,
                Some(ProjectGitRemote {
                    origin_url: Some(ORIGIN.to_string()),
                    repository_root_path: Some(root.to_string_lossy().to_string()),
                }),
            );
            let name = if folder.is_empty() {
                "monorepo"
            } else {
                folder
            };
            let project_id = repository
                .create_project(&params(json!({ "name": name, "path": path_text })))
                .expect("create project")["projectId"]
                .as_str()
                .expect("projectId")
                .to_string();
            (folder.to_string(), project_id)
        })
        .collect()
}

fn id_of<'a>(projects: &'a [(String, String)], folder: &str) -> &'a str {
    &projects
        .iter()
        .find(|(known, _)| known == folder)
        .expect("project")
        .1
}

/// A Spaces document: (space id, name, member project ids, member group ids), in order.
fn spaces_doc(list: &[(&str, &str, Vec<&str>, Vec<&str>)]) -> Value {
    let mut map = Map::new();
    for (id, name, projects, groups) in list {
        map.insert(
            id.to_string(),
            json!({
                "spaceId": id,
                "name": name,
                "icon": "",
                "color": "",
                "memberCollectionIds": groups,
                "memberProjectIds": projects,
            }),
        );
    }
    json!({ "order": list.iter().map(|(id, ..)| *id).collect::<Vec<_>>(), "spaces": map })
}

fn write_spaces(state: &AppState, document: Value) {
    let db = crate::storage::open_gxserver_database(&state.paths).expect("open db");
    update_sidebar_spaces(&db, &params(json!({ "state": document }))).expect("write spaces");
}

/// Each Space's members here, by Space name.
fn members(state: &AppState) -> Vec<(String, Vec<String>)> {
    let db = crate::storage::open_gxserver_database(&state.paths).expect("open db");
    let document = read_sidebar_spaces(&db).expect("read spaces");
    let mut out = Vec::new();
    for id in document["order"].as_array().into_iter().flatten() {
        let space = &document["spaces"][id.as_str().unwrap_or_default()];
        out.push((
            space["name"].as_str().unwrap_or_default().to_string(),
            space["memberProjectIds"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|id| id.as_str().map(str::to_string))
                .collect(),
        ));
    }
    out
}

fn members_of(state: &AppState, name: &str) -> Vec<String> {
    members(state)
        .into_iter()
        .find(|(known, _)| known == name)
        .map(|(_, members)| members)
        .unwrap_or_default()
}

/// What an exchange does on `state` with `incoming` from `caller`, returning its entries.
fn exchange(state: &AppState, incoming: Vec<SyncEntry>, first: bool) -> Vec<SyncEntry> {
    with_engine(state, |engine| {
        engine.export()?;
        engine.merge(incoming, first);
        engine.import()?;
        Ok(engine.outgoing())
    })
    .expect("exchange")
}

#[test]
fn spaces_sync_keys_each_folder_of_a_repository_apart() {
    let home = tempfile::tempdir().expect("tempdir");
    let mac = computer(home.path(), "S7a");
    let projects = monorepo(&mac, home.path(), &["product-x", "trading", ""]);
    let db = crate::storage::open_gxserver_database(&mac.paths).expect("open db");
    let read = read_local_spaces(&db, "S7a", &Default::default()).expect("read");
    assert!(read.remotes_probed);
    let key_of = |folder: &str| {
        read.repos
            .iter()
            .find(|(id, _)| id == id_of(&projects, folder))
            .map(|(_, key)| key.clone())
    };
    assert_eq!(key_of("product-x"), Some(format!("{MONOREPO}#product-x")));
    assert_eq!(key_of("trading"), Some(format!("{MONOREPO}#trading")));
    assert_eq!(key_of(""), Some(MONOREPO.to_string()));
}

#[test]
fn a_monorepo_on_two_computers_keeps_each_folder_in_its_own_space() {
    let mac_home = tempfile::tempdir().expect("tempdir");
    let laptop_home = tempfile::tempdir().expect("tempdir");
    let mac = computer(mac_home.path(), "S7a");
    let laptop = computer(laptop_home.path(), "S8b");
    // The laptop has the same folders, in another order, and has synced on its own before (so
    // its "in no Space" is older than the Mac's arrangement below).
    let on_laptop = monorepo(&laptop, laptop_home.path(), &["trading", "", "product-x"]);
    exchange(&laptop, Vec::new(), false);
    std::thread::sleep(std::time::Duration::from_millis(5));

    let on_mac = monorepo(
        &mac,
        mac_home.path(),
        &["product-x", "trading", "", "loose"],
    );
    write_spaces(
        &mac,
        spaces_doc(&[
            (
                "SPproduct",
                "Product",
                vec![id_of(&on_mac, "product-x")],
                vec![],
            ),
            (
                "SPtrading",
                "Trading",
                vec![id_of(&on_mac, "trading")],
                vec![],
            ),
            ("SPai", "AI", vec![id_of(&on_mac, "")], vec![]),
        ]),
    );
    let before = members(&mac);

    // The Mac's first sync (with no one, as on the reporter's computer with only a phone).
    let mac_entries = exchange(&mac, Vec::new(), true);
    assert_eq!(members(&mac), before, "the first sync moved nothing");

    // The Mac's entries file each of the laptop's folders where the Mac has it.
    let laptop_entries = exchange(&laptop, mac_entries, true);
    assert_eq!(
        members_of(&laptop, "Product"),
        [id_of(&on_laptop, "product-x")]
    );
    assert_eq!(
        members_of(&laptop, "Trading"),
        [id_of(&on_laptop, "trading")]
    );
    assert_eq!(members_of(&laptop, "AI"), [id_of(&on_laptop, "")]);

    // And back: the Mac is unchanged.
    exchange(&mac, laptop_entries, true);
    assert_eq!(members(&mac), before);
}

#[test]
fn spaces_folded_by_11_0_0_are_put_back_from_the_backup_once() {
    let home = tempfile::tempdir().expect("tempdir");
    let mac = computer(home.path(), "S7a");
    let projects = monorepo(
        &mac,
        home.path(),
        &["product-x", "media-v3", "", "trading", "loose"],
    );
    let id = |folder: &str| id_of(&projects, folder).to_string();

    // Before sync first wrote them.
    let original = spaces_doc(&[
        ("SPmedia", "Media", vec![&id("media-v3")], vec![]),
        ("SPproduct", "Product", vec![&id("product-x")], vec![]),
        ("SPai", "AI", vec![&id("")], vec![]),
        ("SPtrading", "Trading", vec![&id("trading")], vec![]),
    ]);
    let backups = mac.paths.root_dir.join("backups");
    std::fs::create_dir_all(&backups).expect("backups");
    std::fs::write(
        backups.join("settings-sync-spaces-20261107-124747.json"),
        serde_json::to_vec(&json!({
            "sidebarSpaces": original,
            "sidebarProjectCollections": { "collections": {}, "order": [], "nextCollectionNumber": 1 },
        }))
        .expect("backup json"),
    )
    .expect("backup");

    // What 11.0.0's sync left: every folder in Product, one entry for the repository, the
    // documents keyed by the repository alone as the baseline.
    let all: Vec<String> = ["product-x", "media-v3", "", "trading", "loose"]
        .iter()
        .map(|folder| id(folder))
        .collect();
    write_spaces(
        &mac,
        spaces_doc(&[
            ("SPmedia", "Media", vec![], vec![]),
            (
                "SPproduct",
                "Product",
                all.iter().map(String::as_str).collect(),
                vec![],
            ),
            ("SPai", "AI", vec![], vec![]),
            ("SPtrading", "Trading", vec![], vec![]),
        ]),
    );
    {
        let db = crate::storage::open_gxserver_database(&mac.paths).expect("open db");
        let mut store = SyncStore::default();
        let mut clock: Option<String> = None;
        let mut put = |key: &str, value: Value| {
            let hlc = next_hlc(clock.as_deref(), 1_790_000_000_000, "S7a");
            clock = Some(hlc.clone());
            store.entries.insert(
                key.to_string(),
                SyncEntry {
                    key: key.to_string(),
                    value: Some(value),
                    hlc,
                    parent: None,
                    origin: "S7a".to_string(),
                    origin_name: "Mac".to_string(),
                },
            );
        };
        for (space, name) in [
            ("SPmedia", "Media"),
            ("SPproduct", "Product"),
            ("SPai", "AI"),
            ("SPtrading", "Trading"),
        ] {
            put(&format!("spaces/space/exists/{space}"), json!(true));
            put(&format!("spaces/space/name/{space}"), json!(name));
        }
        put(
            &format!("spaces/repo/place/{MONOREPO}"),
            json!({ "space": "SPproduct" }),
        );
        store.clock = clock;
        store.spaces_backed_up = true;
        store.spaces_baseline = Some(SpacesBaseline {
            spaces: read_sidebar_spaces(&db).expect("read spaces"),
            collections: read_sidebar_project_collections(&db).expect("read groups"),
            repos: all
                .iter()
                .map(|project| (project.clone(), MONOREPO.to_string()))
                .collect(),
        });
        store.save(&db).expect("save store");
    }

    exchange(&mac, Vec::new(), false);
    assert_eq!(members_of(&mac, "Media"), [id("media-v3")]);
    assert_eq!(members_of(&mac, "Product"), [id("product-x")]);
    assert_eq!(members_of(&mac, "AI"), [id("")]);
    assert_eq!(members_of(&mac, "Trading"), [id("trading")]);

    // Once only: a later fold-like arrangement made by hand stays.
    let by_hand = spaces_doc(&[
        ("SPmedia", "Media", vec![], vec![]),
        (
            "SPproduct",
            "Product",
            all.iter().map(String::as_str).collect(),
            vec![],
        ),
        ("SPai", "AI", vec![], vec![]),
        ("SPtrading", "Trading", vec![], vec![]),
    ]);
    write_spaces(&mac, by_hand);
    exchange(&mac, Vec::new(), false);
    assert_eq!(members_of(&mac, "Product"), all);
}
