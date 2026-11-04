//! `cargo xtask build-mobile-chat-wasm`: the phone's chat core as WebAssembly, inlined into the
//! page the app's hidden WebView loads (docs/2026-10-23/mobile-ota/webview-core/PLAN.md).
//!
//! Builds `packages/gx-chat-wasm` for wasm32-unknown-unknown, shrinks it with binaryen's
//! `wasm-opt -O3`, and writes `apps/mobile/app/assets/chat-core/chat-core.html` from
//! `packages/gx-chat-wasm/page/chat-core.html` with the wasm as base64. The output is a gitignored
//! build product that Metro bundles into the app and its over-the-air updates.

use std::fs;
use std::process::Command;

use crate::bail;
use crate::util::{check, root, Res};

const PLACEHOLDER: &str = "__GX_CHAT_CORE_WASM_BASE64__";

pub fn run(args: &[String]) -> Res<i32> {
    if let Some(arg) = args.first() {
        bail!("`cargo xtask build-mobile-chat-wasm` takes no arguments (got {arg}).");
    }
    let crate_dir = root().join("packages/gx-chat-wasm");
    let app_dir = root().join("apps/mobile/app");
    if !app_dir.join("package.json").exists() {
        bail!(
            "{} is not checked out (git submodule update --init apps/mobile/app).",
            app_dir.display()
        );
    }
    if Command::new("wasm-opt").arg("--version").output().is_err() {
        bail!("wasm-opt (binaryen) is required: `brew install binaryen` on macOS, `apt-get install binaryen` on Linux.");
    }

    // Run from the crate so its rust-toolchain.toml picks the pinned compiler and the wasm target.
    check(Command::new("cargo").current_dir(&crate_dir).args([
        "build",
        "--release",
        "--target",
        "wasm32-unknown-unknown",
    ]))?;
    let built = crate_dir.join("target/wasm32-unknown-unknown/release/gx_chat_wasm.wasm");
    let optimized = crate_dir.join("target/gx_chat_wasm.opt.wasm");
    // wasm-opt validates against the MVP unless told otherwise; these are the features rustc 1.99
    // turns on by default for wasm32-unknown-unknown. Android System WebView 124 and iOS 16.4's
    // WebKit (the app's floor) run all of them.
    check(
        Command::new("wasm-opt")
            .arg("-O3")
            .args(["--strip-debug", "--strip-producers"])
            .args([
                "--enable-sign-ext",
                "--enable-mutable-globals",
                "--enable-nontrapping-float-to-int",
                "--enable-bulk-memory",
                "--enable-reference-types",
                "--enable-multivalue",
            ])
            .arg(&built)
            .arg("-o")
            .arg(&optimized),
    )?;

    let wasm = fs::read(&optimized)?;
    let template = fs::read_to_string(crate_dir.join("page/chat-core.html"))?;
    if template.matches(PLACEHOLDER).count() != 1 {
        bail!("page/chat-core.html must contain {PLACEHOLDER} exactly once.");
    }
    let page = template.replace(PLACEHOLDER, &base64(&wasm));
    let out_dir = app_dir.join("assets/chat-core");
    fs::create_dir_all(&out_dir)?;
    let out = out_dir.join("chat-core.html");
    fs::write(&out, &page)?;
    println!(
        "Wrote {} ({} bytes; wasm {} bytes after wasm-opt, {} before).",
        out.display(),
        page.len(),
        wasm.len(),
        fs::metadata(&built)?.len()
    );
    Ok(0)
}

/// Standard base64 with padding; the page decodes it with `atob`.
fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let triple = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for index in 0..4 {
            if index <= chunk.len() {
                out.push(ALPHABET[((triple >> (18 - 6 * index)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
