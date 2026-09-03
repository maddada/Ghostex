/*
CDXC:Telemetry 2026-08-26:
Bake the SHIPPING marketing version into the gxserver binary, mirroring
`apps/desktop/build.rs`.

`CARGO_PKG_VERSION` for this crate is the placeholder `0.1.0` and has never
tracked releases, so without this every analytics event, and anything else that
wants to know what build it is, would report the same value for every version
Ghostex has ever shipped. The release scripts already resolve the real marketing
version for the desktop crate; they now pass the same value here.

Dev builds have no `GHOSTEX_GPUI_MARKETING_VERSION` and therefore report
`CARGO_PKG_VERSION`, which is exactly what makes them identifiable as dev builds
(`telemetry::base::is_dev_build`).
*/

use std::env;

fn main() {
    println!("cargo:rerun-if-env-changed=GHOSTEX_GPUI_MARKETING_VERSION");
    let marketing_version = env::var("GHOSTEX_GPUI_MARKETING_VERSION")
        .ok()
        .map(|version| version.trim().to_string())
        .filter(|version| !version.is_empty())
        .unwrap_or_else(|| env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION"));
    println!("cargo:rustc-env=GHOSTEX_BUILD_MARKETING_VERSION={marketing_version}");
}
