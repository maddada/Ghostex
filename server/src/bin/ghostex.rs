/*
CDXC:Cli 2026-07-13:
The `ghostex`/`gx` user CLI, ported from scripts/ghostex-cli.mjs into the
gxserver workspace so macOS, Windows, Linux desktops and remote hosts ship one
implementation with no Node runtime. All logic lives in
gxserver::ghostex_cli; this entry point only maps the result to the process
exit code (errors are printed inside run() with the Node CLI's JSON shape).
Releases ship `ghostex` as gxserver under the CLI's name instead of this
binary (`invoked_as_cli` in main.rs; a hard link on Windows). Only
`cargo run --bin ghostex` uses it, and the Windows release still compiles it
because .github/workflows/release-gpui-windows.yml checks and uploads it.
*/
fn main() {
    // `ghostex app show` draws the app's icon in a child of this program (ghostex_apps/icon.rs).
    if std::env::args_os().nth(1).as_deref()
        == Some(std::ffi::OsStr::new(gxserver::DRAW_ICON_COMMAND))
    {
        std::process::exit(gxserver::run_draw_icon_child());
    }
    std::process::exit(gxserver::ghostex_cli::run());
}
