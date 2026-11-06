//! `cargo xtask gbots-runtime`: builds the GBots runtime (OpenMausBot's headless server at the
//! commit pinned in tooling/gbots/upstream.json, without `enterprise/`, renamed GBots) and, with
//! `--install`, puts it where the running gxserver looks for it, which `ghostex gbots status
//! --json` reports (so a test server's `env.sh` installs into that server's own data folder).

use std::path::PathBuf;
use std::process::Command;

use crate::util::{self, Res};

pub fn run(args: &[String]) -> Res<i32> {
    let mut build = Command::new(util::root().join("tooling/gbots/build-runtime.sh"));
    let mut install = false;
    let mut dest: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--install" => install = true,
            "--clean" => {
                build.arg("--clean");
            }
            "--source" => {
                index += 1;
                let source = args
                    .get(index)
                    .ok_or("--source needs a git URL or a clone")?;
                build.args(["--source", source]);
            }
            "--dest" => {
                index += 1;
                dest = Some(PathBuf::from(
                    args.get(index).ok_or("--dest needs a folder")?,
                ));
            }
            other => return Err(format!("unknown argument `{other}`").into()),
        }
        index += 1;
    }
    let output = util::output(build.current_dir(util::root()))?;
    if !output.status.success() {
        return Err(format!(
            "the runtime build failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let built = String::from_utf8_lossy(&output.stdout)
        .lines()
        .last()
        .map(PathBuf::from)
        .ok_or("the runtime build printed no folder")?;
    println!("Built {}", built.display());
    if !install {
        return Ok(0);
    }
    let dest = match dest {
        Some(dest) => dest,
        None => {
            let status = util::output(Command::new("ghostex").args(["gbots", "status", "--json"]))?;
            let status: serde_json::Value =
                serde_json::from_slice(&status.stdout).map_err(|_| {
                    "`ghostex gbots status --json` did not answer; pass --dest <folder>"
                })?;
            PathBuf::from(
                status["runtimeDir"]
                    .as_str()
                    .ok_or("`ghostex gbots status --json` named no runtimeDir; pass --dest")?,
            )
        }
    };
    if dest.exists() {
        std::fs::remove_dir_all(&dest)?;
    }
    std::fs::create_dir_all(dest.parent().ok_or("--dest has no parent")?)?;
    util::check(Command::new("cp").arg("-R").arg(&built).arg(&dest))?;
    println!("Installed {}", dest.display());
    Ok(0)
}
