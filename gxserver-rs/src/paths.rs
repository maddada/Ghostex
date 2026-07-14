use std::{env, path::Path, path::PathBuf};

#[derive(Clone, Debug)]
pub struct AgentPaths {
    pub home_dir: PathBuf,
    pub agents_root: PathBuf,
    pub skills_root: PathBuf,
    pub hooks_root: PathBuf,
    pub profiles_root: PathBuf,
}

impl AgentPaths {
    pub fn new(home_dir: impl AsRef<Path>) -> Self {
        let home_dir = home_dir.as_ref().to_path_buf();
        let agents_root = home_dir.join(".agents");
        Self {
            agents_root: agents_root.clone(),
            skills_root: agents_root.join("skills"),
            hooks_root: home_dir.join(".ghostex").join("hooks"),
            profiles_root: agents_root.join("profiles"),
            home_dir,
        }
    }

    pub fn relative_components(&self) -> Vec<(&str, Vec<&str>)> {
        vec![
            (
                "agents_root",
                components_relative_to_home(&self.home_dir, &self.agents_root),
            ),
            (
                "skills_root",
                components_relative_to_home(&self.home_dir, &self.skills_root),
            ),
            (
                "hooks_root",
                components_relative_to_home(&self.home_dir, &self.hooks_root),
            ),
            (
                "profiles_root",
                components_relative_to_home(&self.home_dir, &self.profiles_root),
            ),
        ]
    }
}

fn components_relative_to_home<'a>(home_dir: &'a Path, path: &'a Path) -> Vec<&'a str> {
    let Ok(relative) = path.strip_prefix(home_dir) else {
        panic!(
            "agent path {} is not under home dir {}",
            path.display(),
            home_dir.display()
        );
    };
    relative
        .components()
        .map(|component| match component {
            std::path::Component::Normal(text) => text.to_str().expect("utf-8 path component"),
            _ => unreachable!("relative path should contain only normal components"),
        })
        .collect()
}

pub fn read_agent_paths_source_ts() -> &'static str {
    include_str!("../../shared/agent-paths.generated.ts")
}

#[derive(Clone, Debug)]
pub struct GxserverPaths {
    pub auth_dir: PathBuf,
    pub auth_token_file: PathBuf,
    pub config_file: PathBuf,
    pub home_dir: PathBuf,
    pub identity_file: PathBuf,
    pub logs_dir: PathBuf,
    pub log_file: PathBuf,
    pub migrations_dir: PathBuf,
    pub portless_state_dir: PathBuf,
    pub root_dir: PathBuf,
    pub runtime_dir: PathBuf,
    pub runtime_metadata_file: PathBuf,
    pub state_db_file: PathBuf,
    pub zmx_dir: PathBuf,
}

/*
CDXC:GxserverStorage 2026-06-14-20:37:
The Rust daemon must use the same durable path contract as the TypeScript source of truth: shared daemon state stays under ~/.ghostex/gxserver, while support-bundle-safe JSONL diagnostics stay under ~/.ghostex/logs.

CDXC:PortlessState 2026-06-22-23:05:
Ghostex-managed Portless state belongs under ~/.ghostex/gxserver/portless, not ~/.portless. gxserver-rs owns this path so the native root service can read mirrored routes while the user daemon remains the only writer.
*/
pub fn get_gxserver_paths(home_dir: Option<PathBuf>) -> GxserverPaths {
    let home_dir = home_dir.unwrap_or_else(default_home_dir);
    let root_dir = home_dir.join(".ghostex").join("gxserver");
    let auth_dir = root_dir.join("auth");
    let logs_dir = home_dir.join(".ghostex").join("logs");
    let migrations_dir = root_dir.join("migrations");
    let portless_state_dir = root_dir.join("portless");
    let runtime_dir = root_dir.join("runtime");
    let zmx_dir = root_dir.join("zmx");

    GxserverPaths {
        auth_token_file: auth_dir.join("token"),
        auth_dir,
        config_file: root_dir.join("config.json"),
        home_dir,
        identity_file: root_dir.join("identity.json"),
        log_file: logs_dir.join("gxserver.jsonl"),
        logs_dir,
        migrations_dir,
        portless_state_dir,
        runtime_metadata_file: runtime_dir.join("server.json"),
        runtime_dir,
        state_db_file: root_dir.join("state.db"),
        zmx_dir,
        root_dir,
    }
}

fn default_home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn agent_paths_resolve_under_home() {
        let home = PathBuf::from("/tmp/agent-paths-test");
        let paths = AgentPaths::new(&home);
        assert!(paths.skills_root.starts_with(&home));
        assert!(paths.hooks_root.starts_with(&home));
        assert!(paths.profiles_root.starts_with(&home));
    }

    #[test]
    fn agent_paths_keep_dot_prefix_for_skills_and_hooks() {
        let home = PathBuf::from("/tmp/agent-paths-test");
        let paths = AgentPaths::new(&home);
        let relative = paths.relative_components();
        let by_name: BTreeMap<_, _> = relative.into_iter().collect();
        assert_eq!(
            by_name["skills_root"],
            vec![".agents", "skills"]
        );
        assert_eq!(
            by_name["hooks_root"],
            vec![".ghostex", "hooks"]
        );
        assert_eq!(
            by_name["agents_root"],
            vec![".agents"]
        );
        assert!(by_name["agents_root"][0].starts_with('.'));
        assert!(by_name["skills_root"][0].starts_with('.'));
        assert!(by_name["hooks_root"][0].starts_with('.'));
    }

    #[test]
    fn generated_ts_matches_rust_agent_paths() {
        let home = PathBuf::from("/tmp/agent-paths-test");
        let paths = AgentPaths::new(&home);
        let ts_source = read_agent_paths_source_ts();
        for (name, components) in paths.relative_components() {
            let ts_constant_name = name.to_ascii_uppercase().replace("_ROOT", "_ROOT");
            let expected = format!(
                "export const {} = \"{}\" as const;",
                ts_constant_name,
                components.join("/")
            );
            assert!(
                ts_source.contains(&expected),
                "generated TS missing expected line: {expected}"
            );
        }
    }
}
