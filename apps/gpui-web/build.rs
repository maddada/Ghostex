//! CDXC:WebGpui 2026-09-22 WHY: some desktop files mix pure drawing helpers with native-only code (`helpers/titlebar.rs` reaches into AppKit, CEF and the file system), so they cannot be compiled here whole, and copying the pure functions would let the two apps drift. This script lifts the named items out of the desktop source at build time instead, byte for byte. `extracted-items.txt` is the list, and doubles as the inventory of what a shared UI crate has to own.
use std::{collections::BTreeMap, env, fs, path::Path};

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
    let desktop = Path::new(&manifest).join("../desktop/src");
    let list_path = Path::new(&manifest).join("extracted-items.txt");
    println!("cargo:rerun-if-changed={}", list_path.display());

    // `<output module> <desktop file> <item> <item> ...` per line.
    let mut outputs: BTreeMap<String, String> = BTreeMap::new();
    for line in fs::read_to_string(&list_path).unwrap().lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut words = line.split_whitespace();
        let output = words.next().unwrap().to_string();
        let file = desktop.join(words.next().unwrap());
        println!("cargo:rerun-if-changed={}", file.display());
        let source =
            fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        let lines: Vec<&str> = source.lines().collect();
        let out = outputs.entry(output).or_default();
        for item in words {
            out.push_str(&extract(&lines, item, &file));
            out.push('\n');
        }
    }
    // libghostty-vt as a static wasm32 archive, built by build-wasm.sh with the same Zig and the same flags the desktop uses (apps/desktop/scripts/build-libghostty-vt.sh) plus `-Dtarget=wasm32-freestanding`. Linked whole so `src/ghostty_vt.rs`, the desktop's FFI wrapper, resolves.
    let vt_archive = Path::new(&manifest).join("target/libghostty-vt-wasm/lib/libghostty-vt.a");
    println!("cargo:rerun-if-changed={}", vt_archive.display());
    if env::var("CARGO_CFG_TARGET_FAMILY").is_ok_and(|family| family.contains("wasm")) {
        assert!(vt_archive.exists(), "run ./build-wasm.sh, which builds {}", vt_archive.display());
        println!("cargo:rustc-link-arg={}", vt_archive.display());
    }

    let out_dir = env::var("OUT_DIR").unwrap();
    for (name, body) in outputs {
        fs::write(Path::new(&out_dir).join(format!("{name}.rs")), body).unwrap();
    }
}

/// Relies on rustfmt's shape: an item starts at column 0 and its block closes with a bare `}` at column 0. `Type::method` names one method of an `impl Type` block (one indent deeper) and comes out wrapped in its own `impl Type`.
fn extract(lines: &[&str], item: &str, file: &Path) -> String {
    if let Some((owner, method)) = item.split_once("::") {
        let indented: Vec<&str> = lines
            .iter()
            .map(|line| line.strip_prefix("    ").unwrap_or(if line.is_empty() { "" } else { "\u{1}" }))
            .collect();
        let body = extract(&indented, method, file);
        return format!("impl {owner} {{\n{body}}}\n");
    }
    // `impl=Type` names the inherent impl block of a type, which shares its name with the type itself.
    let (heads, item): (&[&str], &str) = match item.strip_prefix("impl=") {
        Some(owner) => (&["impl"], owner),
        None => (&["fn", "const", "static", "struct", "enum", "type"], item),
    };
    let start = lines
        .iter()
        .position(|line| {
            let rest = line
                .strip_prefix("pub(crate) ")
                .or_else(|| line.strip_prefix("pub(super) "))
                .or_else(|| line.strip_prefix("pub "))
                .unwrap_or(line);
            heads.iter().any(|head| {
                rest.strip_prefix(head)
                    .and_then(|r| r.strip_prefix(' '))
                    .and_then(|r| r.strip_prefix(item))
                    .is_some_and(|r| !r.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
            })
        })
        .unwrap_or_else(|| panic!("item `{item}` not found in {}", file.display()));
    let mut first = start;
    while first > 0 {
        let above = lines[first - 1].trim_start();
        if above.starts_with("///") || above.starts_with("#[") {
            first -= 1;
        } else {
            break;
        }
    }
    let mut end = start;
    loop {
        let line = lines[end];
        let single_line = end == start && (line.ends_with(';') || line.ends_with('}'));
        let is_value = ["const ", "static ", "type "]
            .iter()
            .any(|head| {
                lines[start]
                    .trim_start_matches("pub(crate) ")
                    .trim_start_matches("pub ")
                    .starts_with(head)
            });
        let closes = end > start
            && (line.starts_with('}')
                || line == "];"
                || line == ");"
                || (is_value && line.ends_with(';') && !line.starts_with("        ")));
        if single_line || closes {
            break;
        }
        end += 1;
    }
    let mut text = lines[first..=end].join("\n");
    text.push('\n');
    text
}
