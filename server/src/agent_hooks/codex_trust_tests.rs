//! Codex 0.167's own trust records, read off a live Codex that was told "Trust all and continue"
//! (2026-11-07, macOS, `--no-daemon` as Ghostex's sessions run it), against what Ghostex writes.

use serde_json::Value;

use super::*;

/// The `hooks.json` that Codex trusted.
const HOOKS: &str = r#"{
  "hooks": {
    "SessionStart": [
      { "hooks": [ { "type": "command", "command": "'/x/agent-shell-notify.sh'", "timeout": 5 } ] }
    ],
    "Stop": [
      { "hooks": [ { "type": "command", "command": "echo stop", "commandWindows": "echo stop-win", "timeout": 5 } ] }
    ],
    "SessionEnd": [
      { "hooks": [ { "type": "command", "command": "echo end", "async": true } ] }
    ],
    "Interrupt": [
      { "hooks": [ { "type": "command", "command": "echo int", "timeout": 3 } ] }
    ],
    "PreToolUse": [
      { "matcher": "Bash", "hooks": [ { "type": "command", "command": "echo pre", "timeout": 5, "statusMessage": "checking" } ] }
    ]
  }
}"#;

/// The `trusted_hash` Codex wrote for each of them.
const CODEX_HASHES: [(&str, &str); 5] = [
    (
        "SessionStart",
        "sha256:047c8cf2637bdc27e7eb5e7c7c6c46f28f7cbd1d790ba51ed3c8ffa0245d2dea",
    ),
    (
        "Stop",
        "sha256:09401cceb4f683a7677cb3c8f8172ff1d9d712e977b65f194022bee07678f972",
    ),
    (
        "SessionEnd",
        "sha256:6c0991f9beecff6c4e3096eaaba4260284ca73c92dde81dc1e8da4b589383b5e",
    ),
    (
        "Interrupt",
        "sha256:0efb1976b12adf458491d6afa4a7a302905e7242fc2226d3835b44486fe3b01e",
    ),
    (
        "PreToolUse",
        "sha256:3c91cbdc993a200b068ca0ed454bbc7732c302d82c18a8144444f0b8ab1b61aa",
    ),
];

#[test]
fn every_hash_is_the_one_codex_writes() {
    let hooks: Value = serde_json::from_str(HOOKS).unwrap();
    for (event, expected) in CODEX_HASHES {
        let group = &hooks["hooks"][event][0];
        let hash = codex_hook_trust_hash(
            event,
            group.get("matcher").and_then(Value::as_str),
            &group["hooks"][0],
        );
        assert_eq!(hash.as_deref(), Some(expected), "{event}");
    }
}

/// Codex keyed the same `hooks.json` by the symlink's path when it was the default `~/.codex`, and
/// by the canonical path when CODEX_HOME named it (both seen live). A temporary folder shows the
/// difference on macOS too: `/var/folders/…` canonicalizes to `/private/var/folders/…`.
#[cfg(unix)]
#[test]
fn the_key_names_the_file_as_codex_found_it() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let real = temp.path().join("real-codex-home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&real).unwrap();
    std::os::unix::fs::symlink(&real, home.join(".codex")).unwrap();
    let profile_link = temp.path().join("profile-link");
    std::os::unix::fs::symlink(&real, &profile_link).unwrap();
    let hook_paths = HookPaths::new(home.clone());
    let canonical_real = path_string(&std::fs::canonicalize(&real).unwrap());

    let default_hooks = home.join(".codex").join("hooks.json");
    assert_eq!(
        codex_hook_key_source(&default_hooks, &hook_paths),
        path_string(&default_hooks),
        "the default home keeps its symlink"
    );
    assert_eq!(
        codex_hook_key_source(&profile_link.join("hooks.json"), &hook_paths),
        format!("{canonical_real}/hooks.json"),
        "a CODEX_HOME is canonicalized"
    );
    // CODEX_HOME set to the default folder is still a CODEX_HOME to Codex.
    assert_eq!(
        codex_hook_key_source_with(&default_hooks, &hook_paths, true, false, |path| {
            std::fs::canonicalize(path)
                .ok()
                .map(|path| path_string(&path))
        }),
        format!("{canonical_real}/hooks.json"),
    );
}

/// The same rules on Windows-shaped homes, where Ghostex used to canonicalize the default home
/// too: a profile folder behind a junction, or spelled in another case on disk, gave every Ghostex
/// hook a key Codex never reads.
#[test]
fn windows_keys_follow_codex() {
    let hook_paths = HookPaths::new(PathBuf::from("C:/Users/Archana"));
    let default_hooks = Path::new("C:/Users/Archana/.codex/hooks.json");
    for canonical in [
        r"\\?\D:\Profiles\Archana\.codex",
        r"\\?\C:\Users\archana\.codex",
        r"\\?\C:\Users\ARCHAN~1\.codex",
    ] {
        assert_eq!(
            codex_hook_key_source_with(default_hooks, &hook_paths, false, true, |_| Some(
                canonical.to_string()
            )),
            r"C:\Users\Archana\.codex\hooks.json",
            "the default home is not canonicalized ({canonical})"
        );
    }
    let cases = [
        (
            "C:/Users/Archana/.codex-profiles/2/hooks.json",
            false,
            r"\\?\C:\Users\Archana\.codex-profiles\2",
            r"C:\Users\Archana\.codex-profiles\2\hooks.json",
        ),
        (
            "C:/Users/Archana/.codex/hooks.json",
            true,
            r"\\?\D:\Profiles\Archana\.codex",
            r"D:\Profiles\Archana\.codex\hooks.json",
        ),
        (
            "//fs01/home$/archana/.codex/hooks.json",
            false,
            r"\\?\UNC\fs01\home$\archana\.codex",
            r"\\fs01\home$\archana\.codex\hooks.json",
        ),
    ];
    for (hooks, codex_home_set, canonical, expected) in cases {
        assert_eq!(
            codex_hook_key_source_with(Path::new(hooks), &hook_paths, codex_home_set, true, |_| {
                Some(canonical.to_string())
            }),
            expected,
            "{hooks}"
        );
    }
    assert_eq!(
        windows_absolute_spelling("C:/Users//Archana/./tmp/../.codex/"),
        r"C:\Users\Archana\.codex"
    );
}
