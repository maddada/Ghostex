use std::{env, path::PathBuf};

use gxserver::paths::AgentPaths;


fn main() {
    let home_dir = env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp/fake-home"));
    let paths = AgentPaths::new(&home_dir);
    let components = paths.relative_components();
    let output: std::collections::BTreeMap<String, Vec<String>> = components
        .into_iter()
        .map(|(name, parts)| (name.to_string(), parts.into_iter().map(String::from).collect()))
        .collect();
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}
