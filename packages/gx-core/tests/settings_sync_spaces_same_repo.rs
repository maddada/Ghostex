//! Issue #488: Spaces sync keyed a project by its repository alone, so every folder project of one
//! monorepo shared one `repo/place` entry and the first sync filed all of them in the Space of
//! the first one. Spaces sync now keys a project by its repository and its folder inside it (the
//! #461 rule of the one list), and `repair_folded_spaces` puts back a repository that was folded.

use std::collections::BTreeMap;

use ghostex_gx_core::settings_sync::{
    apply_spaces_entries, derive_spaces_edits, local_edit, merge_entries, next_hlc,
    repair_folded_spaces, LocalSpaces, SyncEntry,
};
use ghostex_gx_core::unified_sidebar::project_repo_key;
use serde_json::{json, Value};

const MONOREPO: &str = "github.com/me/monorepo";

fn folder_key(folder: Option<&str>) -> String {
    project_repo_key(MONOREPO, folder)
}

/// A Spaces document: (space id, name, member project ids, member group ids), in order.
fn spaces(list: &[(&str, &str, &[&str], &[&str])]) -> Value {
    let mut map = serde_json::Map::new();
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
    json!({
        "order": list.iter().map(|(id, ..)| *id).collect::<Vec<_>>(),
        "spaces": map,
    })
}

/// A groups document: (group id, title, project ids), in order.
fn groups(list: &[(&str, &str, &[&str])]) -> Value {
    let mut map = serde_json::Map::new();
    for (id, title, projects) in list {
        map.insert(
            id.to_string(),
            json!({
                "collectionId": id,
                "title": title,
                "color": "",
                "projectIds": projects,
            }),
        );
    }
    json!({
        "collections": map,
        "order": list.iter().map(|(id, ..)| *id).collect::<Vec<_>>(),
        "nextCollectionNumber": list.len() + 1,
    })
}

fn members(document: &Value, space: &str) -> Vec<String> {
    document["spaces"][space]["memberProjectIds"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|id| id.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// One computer's side of sync: its entries and clock.
struct Computer {
    id: &'static str,
    entries: BTreeMap<String, SyncEntry>,
    clock: Option<String>,
    now_ms: u64,
}

impl Computer {
    fn new(id: &'static str) -> Self {
        Self {
            id,
            entries: BTreeMap::new(),
            clock: None,
            now_ms: 1_790_000_000_000,
        }
    }

    /// Takes this computer's edits since `baseline` into its entries, as gxserver's export does.
    fn export(&mut self, current: LocalSpaces<'_>, baseline: Option<LocalSpaces<'_>>) {
        for (key, value) in derive_spaces_edits(current, baseline, &self.entries) {
            self.now_ms += 1;
            let stamp = next_hlc(self.clock.as_deref(), self.now_ms, self.id);
            if let Some(entry) =
                local_edit(&self.entries, &key, value, stamp.clone(), self.id, self.id)
            {
                self.entries.insert(key, entry);
                self.clock = Some(stamp);
            }
        }
    }

    fn take(&mut self, other: &Computer) {
        merge_entries(&mut self.entries, other.entries.values().cloned());
        self.clock = self.clock.clone().max(other.clock.clone());
        self.now_ms = self.now_ms.max(other.now_ms);
    }
}

/// The reporter's layout, reduced: the monorepo's root and four of its folders in four Spaces,
/// one folder in no Space, and a project with its own repository.
fn reporter_spaces() -> Value {
    spaces(&[
        ("media", "Media", &["P-media", "P-own"], &[]),
        ("product", "Product", &["P-product"], &[]),
        ("ai", "AI", &["P-root", "P-ai"], &[]),
        ("trading", "Trading", &["P-trading"], &[]),
    ])
}

/// Project id → key, in the project list's order (product-x first, as in the report).
fn reporter_repos(by_folder: bool) -> Vec<(String, String)> {
    let key = |folder: Option<&str>| {
        if by_folder {
            folder_key(folder)
        } else {
            MONOREPO.to_string()
        }
    };
    vec![
        ("P-product".to_string(), key(Some("product-x"))),
        ("P-media".to_string(), key(Some("media-v3"))),
        ("P-root".to_string(), key(None)),
        ("P-ai".to_string(), key(Some("ai"))),
        ("P-trading".to_string(), key(Some("trading"))),
        ("P-loose".to_string(), key(Some("loose"))),
        ("P-own".to_string(), "github.com/me/own".to_string()),
    ]
}

/// The first sync on one computer: export with no baseline, then write what the entries say.
fn first_sync(
    computer: &mut Computer,
    spaces: &Value,
    groups: &Value,
    repos: &[(String, String)],
) -> Value {
    let local = LocalSpaces {
        spaces,
        collections: groups,
        repos,
    };
    computer.export(local, None);
    apply_spaces_entries(&computer.entries, local).spaces
}

#[test]
fn keyed_by_repository_alone_the_first_sync_folds_every_folder_into_one_space() {
    // What 11.0.0 did, kept as the cause this fix removes.
    let mut computer = Computer::new("mac");
    let empty = groups(&[]);
    let applied = first_sync(
        &mut computer,
        &reporter_spaces(),
        &empty,
        &reporter_repos(false),
    );
    assert_eq!(
        members(&applied, "product"),
        [
            "P-product",
            "P-media",
            "P-root",
            "P-ai",
            "P-trading",
            "P-loose"
        ]
    );
    assert!(members(&applied, "trading").is_empty());
}

#[test]
fn keyed_by_repository_and_folder_the_first_sync_leaves_every_space_as_it_is() {
    let mut computer = Computer::new("mac");
    let empty = groups(&[]);
    let before = reporter_spaces();
    let applied = first_sync(&mut computer, &before, &empty, &reporter_repos(true));
    assert_eq!(applied, before);
    // The loose folder is filed nowhere, by its own key.
    assert_eq!(
        computer.entries[&format!("spaces/repo/place/{}", folder_key(Some("loose")))].value,
        Some(Value::Null)
    );
}

#[test]
fn a_monorepo_on_two_computers_files_each_folder_in_its_own_space_on_both() {
    // The Mac's layout, synced first.
    let mut mac = Computer::new("mac");
    let empty = groups(&[]);
    let mac_spaces = reporter_spaces();
    let mac_repos = reporter_repos(true);
    assert_eq!(
        first_sync(&mut mac, &mac_spaces, &empty, &mac_repos),
        mac_spaces
    );

    // The laptop has the same monorepo checked out (other project ids, product-x listed last) and
    // only Spaces that came from the Mac, with nothing filed in them yet.
    let laptop_repos = vec![
        ("L-trading".to_string(), folder_key(Some("trading"))),
        ("L-root".to_string(), folder_key(None)),
        ("L-ai".to_string(), folder_key(Some("ai"))),
        ("L-media".to_string(), folder_key(Some("media-v3"))),
        ("L-product".to_string(), folder_key(Some("product-x"))),
    ];
    let mut laptop = Computer::new("laptop");
    laptop.take(&mac);
    let laptop_spaces = spaces(&[
        ("media", "Media", &[], &[]),
        ("product", "Product", &[], &[]),
        ("ai", "AI", &[], &[]),
        ("trading", "Trading", &[], &[]),
    ]);
    let laptop_local = LocalSpaces {
        spaces: &laptop_spaces,
        collections: &empty,
        repos: &laptop_repos,
    };
    // Its baseline is what it had, so it has no edits of its own to send.
    laptop.export(laptop_local, Some(laptop_local));
    let applied = apply_spaces_entries(&laptop.entries, laptop_local).spaces;
    assert_eq!(members(&applied, "media"), ["L-media"]);
    assert_eq!(members(&applied, "product"), ["L-product"]);
    assert_eq!(members(&applied, "ai"), ["L-root", "L-ai"]);
    assert_eq!(members(&applied, "trading"), ["L-trading"]);

    // A move on the laptop reaches the Mac for that folder only.
    let moved = spaces(&[
        ("media", "Media", &["L-media"], &[]),
        ("product", "Product", &["L-product"], &[]),
        ("ai", "AI", &["L-root"], &[]),
        ("trading", "Trading", &["L-trading", "L-ai"], &[]),
    ]);
    let applied_local = LocalSpaces {
        spaces: &applied,
        collections: &empty,
        repos: &laptop_repos,
    };
    laptop.export(
        LocalSpaces {
            spaces: &moved,
            collections: &empty,
            repos: &laptop_repos,
        },
        Some(applied_local),
    );
    mac.take(&laptop);
    let mac_applied = apply_spaces_entries(
        &mac.entries,
        LocalSpaces {
            spaces: &mac_spaces,
            collections: &empty,
            repos: &mac_repos,
        },
    )
    .spaces;
    assert_eq!(members(&mac_applied, "media"), ["P-media", "P-own"]);
    assert_eq!(members(&mac_applied, "product"), ["P-product"]);
    assert_eq!(members(&mac_applied, "ai"), ["P-root"]);
    assert_eq!(members(&mac_applied, "trading"), ["P-trading", "P-ai"]);
}

/// What 11.0.0's first sync left the reporter's computer with: every monorepo folder in Product,
/// the Health group (monorepo/health) dropped once empty.
fn folded() -> (Value, Value, Value, Value) {
    let backup_spaces = spaces(&[
        ("media", "Media", &["P-media", "P-own"], &[]),
        ("product", "Product", &["P-product"], &[]),
        ("ai", "AI", &["P-root", "P-ai"], &["health"]),
        ("trading", "Trading", &["P-trading"], &[]),
    ]);
    let backup_groups = groups(&[("health", "Health", &["P-health"])]);
    let folded_spaces = spaces(&[
        ("media", "Media", &["P-own"], &[]),
        (
            "product",
            "Product",
            &[
                "P-product",
                "P-media",
                "P-root",
                "P-ai",
                "P-trading",
                "P-loose",
                "P-health",
            ],
            &[],
        ),
        ("ai", "AI", &[], &[]),
        ("trading", "Trading", &[], &[]),
    ]);
    (backup_spaces, backup_groups, folded_spaces, groups(&[]))
}

fn folded_repos(by_folder: bool) -> Vec<(String, String)> {
    let mut repos = reporter_repos(by_folder);
    let key = if by_folder {
        folder_key(Some("health"))
    } else {
        MONOREPO.to_string()
    };
    repos.push(("P-health".to_string(), key));
    repos
}

#[test]
fn the_repair_puts_a_folded_monorepo_back_from_the_backup_and_sync_keeps_it() {
    let (backup_spaces, backup_groups, folded_spaces, folded_groups) = folded();
    let repos = folded_repos(true);
    let repaired = repair_folded_spaces(
        LocalSpaces {
            spaces: &folded_spaces,
            collections: &folded_groups,
            repos: &repos,
        },
        &backup_spaces,
        &backup_groups,
    )
    .expect("the folded monorepo is put back");
    assert_eq!(members(&repaired.spaces, "media"), ["P-media", "P-own"]);
    assert_eq!(members(&repaired.spaces, "product"), ["P-product"]);
    assert_eq!(members(&repaired.spaces, "ai"), ["P-root", "P-ai"]);
    assert_eq!(members(&repaired.spaces, "trading"), ["P-trading"]);
    // The group the fold emptied comes back in its Space, with its project.
    assert_eq!(
        repaired.collections["collections"]["health"]["projectIds"],
        json!(["P-health"])
    );
    assert_eq!(
        repaired.spaces["spaces"]["ai"]["memberCollectionIds"],
        json!(["health"])
    );

    // The next sync step: the entries still hold 11.0.0's fold under the bare key, and the
    // baseline is the folded documents keyed the old way. Exporting the repaired documents and
    // writing what the entries then say keeps them.
    let mut computer = Computer::new("mac");
    let old_repos = folded_repos(false);
    let folded_local = LocalSpaces {
        spaces: &folded_spaces,
        collections: &folded_groups,
        repos: &old_repos,
    };
    computer.export(folded_local, None);
    let repaired_local = LocalSpaces {
        spaces: &repaired.spaces,
        collections: &repaired.collections,
        repos: &repos,
    };
    computer.export(repaired_local, Some(folded_local));
    let applied = apply_spaces_entries(&computer.entries, repaired_local);
    assert_eq!(applied.spaces["spaces"], repaired.spaces["spaces"]);
    assert_eq!(
        applied.collections["collections"],
        repaired.collections["collections"]
    );
}

#[test]
fn the_repair_leaves_a_monorepo_the_user_has_spread_over_spaces_alone() {
    let (backup_spaces, backup_groups, _, _) = folded();
    // Rearranged by hand since (not all in one place): nothing to undo.
    let arranged = spaces(&[
        ("media", "Media", &["P-own", "P-media"], &[]),
        (
            "product",
            "Product",
            &["P-product", "P-loose", "P-health"],
            &[],
        ),
        ("ai", "AI", &["P-ai"], &[]),
        ("trading", "Trading", &["P-trading", "P-root"], &[]),
    ]);
    let empty = groups(&[]);
    let repos = folded_repos(true);
    assert_eq!(
        repair_folded_spaces(
            LocalSpaces {
                spaces: &arranged,
                collections: &empty,
                repos: &repos,
            },
            &backup_spaces,
            &backup_groups,
        ),
        None
    );
}

#[test]
fn the_repair_moves_nothing_into_a_space_deleted_since_and_leaves_other_repositories() {
    let (backup_spaces, backup_groups, _, folded_groups) = folded();
    // Trading was deleted after the fold; P-own (its own repository) was moved to Product.
    let folded_spaces = spaces(&[
        ("media", "Media", &[], &[]),
        (
            "product",
            "Product",
            &[
                "P-product",
                "P-media",
                "P-root",
                "P-ai",
                "P-trading",
                "P-loose",
                "P-health",
                "P-own",
            ],
            &[],
        ),
        ("ai", "AI", &[], &[]),
    ]);
    let repos = folded_repos(true);
    let repaired = repair_folded_spaces(
        LocalSpaces {
            spaces: &folded_spaces,
            collections: &folded_groups,
            repos: &repos,
        },
        &backup_spaces,
        &backup_groups,
    )
    .expect("the folded monorepo is put back");
    assert_eq!(members(&repaired.spaces, "media"), ["P-media"]);
    assert_eq!(
        members(&repaired.spaces, "product"),
        ["P-product", "P-trading", "P-own"]
    );
    assert_eq!(members(&repaired.spaces, "ai"), ["P-root", "P-ai"]);
}

#[test]
fn a_repository_with_one_project_is_never_repaired() {
    let backup = spaces(&[("a", "A", &["P1"], &[]), ("b", "B", &[], &[])]);
    let now = spaces(&[("a", "A", &[], &[]), ("b", "B", &["P1"], &[])]);
    let empty = groups(&[]);
    let repos = vec![("P1".to_string(), folder_key(Some("only")))];
    assert_eq!(
        repair_folded_spaces(
            LocalSpaces {
                spaces: &now,
                collections: &empty,
                repos: &repos,
            },
            &backup,
            &empty,
        ),
        None
    );
}
