use std::{
    env,
    path::{Path, PathBuf},
};

/*
CDXC:PlatformSupport 2026-06-23-07:52:
Ubuntu packaging must share the macOS gxserver resource contract instead of adding PATH fallbacks. Resolve bundled Node, Portless, and tool roots from package-relative layouts first, then keep development-only source candidates explicit for local validation.
*/
pub fn code_server_node_candidates() -> Vec<PathBuf> {
    resource_candidates("code-server/lib/node")
}

pub fn portless_cli_candidates() -> Vec<PathBuf> {
    resource_candidates("portless/dist/cli.js")
}

pub fn source_web_resource(relative: &str) -> Option<PathBuf> {
    env::current_dir().ok().map(|cwd| {
        cwd.join("native")
            .join("macos")
            .join("ghostexHost")
            .join("Web")
            .join(relative)
    })
}

fn resource_candidates(relative: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(current_exe) = env::current_exe() {
        if let Some(web_root) = web_root_from_packaged_executable(&current_exe) {
            candidates.push(web_root.join(relative));
        }
        for ancestor in current_exe.ancestors() {
            candidates.push(ancestor.join("Web").join(relative));
            candidates.push(ancestor.join(relative));
            candidates.push(
                ancestor
                    .join("native")
                    .join("macos")
                    .join("ghostexHost")
                    .join("Web")
                    .join(relative),
            );
        }
    }
    if let Some(source) = source_web_resource(relative) {
        candidates.push(source);
    }
    dedupe_paths(candidates)
}

fn web_root_from_packaged_executable(executable_path: &Path) -> Option<PathBuf> {
    executable_path
        .parent()
        .and_then(|bin_dir| {
            (bin_dir.file_name().and_then(|name| name.to_str()) == Some("bin")).then_some(bin_dir)
        })
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(Path::to_path_buf)
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    paths
        .into_iter()
        .filter(|path| {
            let key = path.components().collect::<PathBuf>();
            seen.insert(key)
        })
        .collect()
}
