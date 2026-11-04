//! `ghostex box work-status` (D13): uncommitted or unpushed work in the box's checkouts keeps the
//! box from being deleted until the user pushes it or deletes anyway.

use std::path::Path;

use ghostex_gx_protocol::cloud_box::{BoxCheckoutWork, BoxWorkStatus};

use super::git::{git, is_repository};
use super::state;

fn checkout_work(path: &Path, placed: &state::Placed) -> BoxCheckoutWork {
    let uncommitted_files = git(path, &["status", "--porcelain"])
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.get(3..))
        .filter(|file| {
            let absolute = path.join(file.trim_matches('"'));
            !placed
                .secret_files
                .iter()
                .any(|secret| Path::new(&secret.path) == absolute)
        })
        .count() as u64;
    // Commits no remote has. A branch with no upstream that adds nothing to what was fetched
    // counts zero; "all of the branch's history" would make every fresh branch look unpushed.
    let unpushed_commits = git(path, &["rev-list", "--count", "HEAD", "--not", "--remotes"])
        .ok()
        .and_then(|count| count.parse::<u64>().ok())
        .unwrap_or(0);
    BoxCheckoutWork {
        path: path.to_string_lossy().into_owned(),
        branch: git(path, &["rev-parse", "--abbrev-ref", "HEAD"])
            .ok()
            .filter(|name| name != "HEAD"),
        uncommitted_files,
        unpushed_commits,
    }
}

pub(crate) fn run_work_status() -> Result<BoxWorkStatus, String> {
    state::require_ticket()?;
    let placed = state::read_placed();
    let checkouts: Vec<BoxCheckoutWork> = placed
        .checkouts
        .iter()
        .map(Path::new)
        .filter(|path| is_repository(path))
        .map(|path| checkout_work(path, &placed))
        .collect();
    Ok(BoxWorkStatus {
        has_unpushed_work: checkouts
            .iter()
            .any(|work| work.uncommitted_files > 0 || work.unpushed_commits > 0),
        checkouts,
    })
}
