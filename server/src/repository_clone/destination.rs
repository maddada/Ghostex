#[cfg(test)]
use std::{collections::HashMap, sync::Arc, time::Duration};
use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::Value;
#[cfg(test)]
use serde_json::{json, Map};
#[cfg(test)]
use tokio::{sync::Mutex, time::sleep};

use super::*;

#[derive(Debug)]
pub(super) struct DestinationStatus {
    pub(super) exists: bool,
    pub(super) is_empty: Option<bool>,
    pub(super) kind: Option<String>,
}

pub(super) fn read_destination_status(
    destination_path: &str,
) -> Result<DestinationStatus, RepositoryCloneError> {
    match fs::symlink_metadata(destination_path) {
        Ok(metadata) => {
            if metadata.is_dir() {
                let is_empty = fs::read_dir(destination_path)
                    .map_err(|error| RepositoryCloneError::bad_request(error.to_string()))?
                    .next()
                    .is_none();
                Ok(DestinationStatus {
                    exists: true,
                    is_empty: Some(is_empty),
                    kind: Some("directory".to_string()),
                })
            } else {
                Ok(DestinationStatus {
                    exists: true,
                    is_empty: None,
                    kind: Some(if metadata.is_file() { "file" } else { "other" }.to_string()),
                })
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(DestinationStatus {
            exists: false,
            is_empty: None,
            kind: None,
        }),
        Err(error) => Err(RepositoryCloneError::bad_request(error.to_string())),
    }
}

pub(super) fn read_required_string(
    input: Option<&Value>,
    field: &str,
) -> Result<String, RepositoryCloneError> {
    let text = input.and_then(Value::as_str).map(str::trim).unwrap_or("");
    if text.is_empty() {
        return Err(RepositoryCloneError::bad_request(format!(
            "{field} must be a non-empty string."
        )));
    }
    Ok(text.to_string())
}

pub(super) fn read_job_id(input: Option<&Value>) -> Result<String, RepositoryCloneError> {
    read_required_string(input, "jobId")
}

pub(super) fn normalize_existing_directory_path(
    input: Option<&Value>,
    field: &str,
) -> Result<String, RepositoryCloneError> {
    let path = normalize_absolute_path(input, field)?;
    let metadata = fs::metadata(&path)
        .map_err(|_| RepositoryCloneError::not_found(format!("{field} does not exist: {path}")))?;
    if metadata.is_dir() {
        Ok(path)
    } else {
        Err(RepositoryCloneError::bad_request(format!(
            "{field} is not a directory: {path}"
        )))
    }
}

pub(super) fn normalize_absolute_path(
    input: Option<&Value>,
    field: &str,
) -> Result<String, RepositoryCloneError> {
    let text = input.and_then(Value::as_str).map(str::trim).unwrap_or("");
    if text.is_empty() {
        return Err(RepositoryCloneError::bad_request(format!(
            "{field} must be a non-empty path."
        )));
    }
    let expanded = expand_user_path(text);
    if !expanded.is_absolute() {
        return Err(RepositoryCloneError::bad_request(format!(
            "{field} must be an absolute path or start with ~/"
        )));
    }
    Ok(normalize_path_string(expanded))
}

fn expand_user_path(input: &str) -> PathBuf {
    if input == "~" {
        return home_dir();
    }
    if let Some(rest) = input.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(input)
}

/// Windows usually has USERPROFILE and no HOME; same home as Add Project's registration.
fn home_dir() -> PathBuf {
    ghostex_paths::GhostexPaths::resolve().home_dir
}

/// CDXC:AddProject 2026-10-09 WHY: the root is the platform separator, never a literal `/`: on
/// native Windows `/` after the `c:` prefix produced `c:/dev\cozy-studio`, which the review
/// step showed and the cloned project was registered under. Drive letters are upper-cased so a
/// typed `c:` matches the `C:\` paths Windows itself reports.
pub(super) fn normalize_path_string(path: impl AsRef<Path>) -> String {
    let mut normalized = PathBuf::new();
    for component in path.as_ref().components() {
        match component {
            std::path::Component::Prefix(prefix) => match prefix.kind() {
                std::path::Prefix::Disk(letter) => {
                    normalized.push(format!("{}:", letter.to_ascii_uppercase() as char))
                }
                _ => normalized.push(prefix.as_os_str()),
            },
            std::path::Component::RootDir => normalized.push(component.as_os_str()),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push("..");
                }
            }
            std::path::Component::Normal(segment) => normalized.push(segment),
        }
    }
    normalized.to_string_lossy().to_string()
}

pub(super) fn is_path_inside(parent_path: &str, candidate_path: &str) -> bool {
    let parent = Path::new(parent_path);
    let candidate = Path::new(candidate_path);
    parent == candidate || candidate.starts_with(parent)
}

pub(super) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub(super) trait NonEmptyString {
    fn or_else(self, fallback: &str) -> String;
}

impl NonEmptyString for String {
    fn or_else(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn preview_repository_clone_accepts_remote_url_and_destination_path() {
        let dir = tempdir().unwrap();
        let destination = dir.path().join("nested").join("my-app");
        let mut params = Map::new();
        params.insert(
            "remoteUrl".to_string(),
            json!("git@github.com:factory-ai/ghostex.git"),
        );
        params.insert(
            "destinationPath".to_string(),
            json!(destination.to_string_lossy()),
        );
        let preview = preview_repository_clone(&params).unwrap();
        assert_eq!(
            preview.get("cloneUrl").and_then(Value::as_str),
            Some("git@github.com:factory-ai/ghostex.git")
        );
        assert_eq!(
            preview.get("destinationFolderName").and_then(Value::as_str),
            Some("my-app")
        );
        assert_eq!(
            preview.get("destinationPath").and_then(Value::as_str),
            Some(normalize_path_string(&destination).as_str())
        );
        assert_eq!(
            preview.get("parentPath").and_then(Value::as_str),
            Some(normalize_path_string(dir.path().join("nested")).as_str())
        );
        assert_eq!(
            preview.get("destinationBlocked").and_then(Value::as_bool),
            Some(false)
        );
        assert!(preview.get("warning").is_none());
    }

    #[test]
    fn preview_repository_clone_uses_an_empty_destination_and_appends_to_a_populated_one() {
        let dir = tempdir().unwrap();
        let empty = dir.path().join("empty-destination");
        fs::create_dir_all(&empty).unwrap();
        let populated = dir.path().join("populated-destination");
        fs::create_dir_all(&populated).unwrap();
        fs::write(populated.join("README.md"), "occupied\n").unwrap();

        let mut params = Map::new();
        params.insert(
            "remoteUrl".to_string(),
            json!("git@github.com:factory-ai/ghostex.git"),
        );
        params.insert(
            "destinationPath".to_string(),
            json!(empty.to_string_lossy()),
        );
        let preview = preview_repository_clone(&params).unwrap();
        assert_eq!(
            preview.get("destinationExists").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            preview.get("destinationBlocked").and_then(Value::as_bool),
            Some(false)
        );

        params.insert(
            "destinationPath".to_string(),
            json!(populated.to_string_lossy()),
        );
        let nested = preview_repository_clone(&params).unwrap();
        assert_eq!(
            nested.get("destinationBlocked").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            nested.get("destinationPath").and_then(Value::as_str),
            Some(normalize_path_string(populated.join("ghostex")).as_str())
        );
        assert_eq!(
            nested.get("parentPath").and_then(Value::as_str),
            Some(normalize_path_string(&populated).as_str())
        );
        assert_eq!(
            nested.get("destinationFolderName").and_then(Value::as_str),
            Some("ghostex")
        );
        assert!(nested.get("warning").is_none());

        fs::create_dir_all(populated.join("ghostex")).unwrap();
        fs::write(populated.join("ghostex").join("README.md"), "occupied\n").unwrap();
        let blocked = preview_repository_clone(&params).unwrap();
        assert_eq!(
            blocked.get("destinationBlocked").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            blocked.get("warning").and_then(Value::as_str),
            Some("Destination path already exists and is not empty.")
        );

        let file_destination = dir.path().join("file-destination");
        fs::write(&file_destination, "file\n").unwrap();
        params.insert(
            "destinationPath".to_string(),
            json!(file_destination.to_string_lossy()),
        );
        let file_blocked = preview_repository_clone(&params).unwrap();
        assert_eq!(
            file_blocked
                .get("destinationBlocked")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            file_blocked.get("warning").and_then(Value::as_str),
            Some("Destination path already exists and is not a directory.")
        );
    }

    #[test]
    fn preview_repository_clone_keeps_the_parent_path_shape_blocking_on_any_existing_destination() {
        let dir = tempdir().unwrap();
        let existing = dir.path().join("ghostex");
        fs::create_dir_all(&existing).unwrap();
        let mut params = Map::new();
        params.insert(
            "repositoryInput".to_string(),
            json!("https://github.com/factory-ai/ghostex"),
        );
        params.insert(
            "parentPath".to_string(),
            json!(dir.path().to_string_lossy()),
        );
        let preview = preview_repository_clone(&params).unwrap();
        assert_eq!(
            preview.get("destinationBlocked").and_then(Value::as_bool),
            Some(true)
        );
        assert!(preview
            .get("warning")
            .and_then(Value::as_str)
            .is_some_and(|warning| warning.starts_with("A directory already exists at ")));
    }

    #[test]
    fn preview_repository_clone_normalizes_github_browser_url() {
        let dir = tempdir().unwrap();
        let mut params = Map::new();
        params.insert(
            "repositoryInput".to_string(),
            json!("https://github.com/factory-ai/ghostex/tree/main"),
        );
        params.insert(
            "parentPath".to_string(),
            json!(dir.path().to_string_lossy()),
        );
        params.insert("shallowClone".to_string(), json!(true));
        let preview = preview_repository_clone(&params).unwrap();
        assert_eq!(
            preview.get("cloneUrl").and_then(Value::as_str),
            Some("https://github.com/factory-ai/ghostex.git")
        );
        assert_eq!(
            preview.get("destinationFolderName").and_then(Value::as_str),
            Some("ghostex")
        );
        assert_eq!(
            preview.get("shallowClone").and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn preview_repository_clone_matches_typescript_url_path_and_name_edges() {
        let dir = tempdir().unwrap();
        let parent = dir.path().join("parent");
        fs::create_dir(&parent).unwrap();
        let parent_with_segments = parent.join("..").join("parent");
        let mut params = Map::new();
        params.insert(
            "repositoryInput".to_string(),
            json!("HTTPS://GitHub.com/factory-ai/ghost%65x.git/blob/main?tab=readme"),
        );
        params.insert(
            "parentPath".to_string(),
            json!(parent_with_segments.to_string_lossy()),
        );
        params.insert(
            "destinationFolderName".to_string(),
            json!("repo//copy: final\\name"),
        );
        let preview = preview_repository_clone(&params).unwrap();
        assert_eq!(
            preview.get("cloneUrl").and_then(Value::as_str),
            Some("https://GitHub.com/factory-ai/ghostex.git")
        );
        assert_eq!(
            preview.get("defaultFolderName").and_then(Value::as_str),
            Some("ghostex")
        );
        assert_eq!(
            preview.get("destinationFolderName").and_then(Value::as_str),
            Some("repo-copy- final-name")
        );
        assert_eq!(
            preview.get("parentPath").and_then(Value::as_str),
            Some(parent.to_string_lossy().as_ref())
        );
        assert_eq!(
            preview.get("destinationPath").and_then(Value::as_str),
            Some(
                parent
                    .join("repo-copy- final-name")
                    .to_string_lossy()
                    .as_ref()
            )
        );
    }

    #[test]
    fn repository_clone_input_parsing_matches_typescript_token_edges() {
        let ssh =
            parse_repository_clone_input("ssh://git@GitHub.com/Factory-AI/Ghost%65x.GIT/tree/main")
                .unwrap();
        assert_eq!(ssh.clone_url, "ssh://git@GitHub.com/Factory-AI/Ghostex.git");
        assert_eq!(ssh.repository_name, "Ghostex");

        let scheme_without_dotted_host =
            parse_repository_clone_input("http://internal/repo").unwrap();
        assert_eq!(
            scheme_without_dotted_host.clone_url,
            "https://github.com/http:/internal/repo.git"
        );

        assert!(parse_repository_clone_input("owner//repo").is_none());
    }

    #[test]
    fn preview_rejects_existing_destination() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("repo")).unwrap();
        let mut params = Map::new();
        params.insert("repositoryInput".to_string(), json!("owner/repo"));
        params.insert(
            "parentPath".to_string(),
            json!(dir.path().to_string_lossy()),
        );
        let preview = preview_repository_clone(&params).unwrap();
        assert_eq!(
            preview.get("destinationExists").and_then(Value::as_bool),
            Some(true)
        );
        assert!(preview
            .get("warning")
            .and_then(Value::as_str)
            .unwrap()
            .contains("already exists"));
    }

    #[test]
    fn repository_branch_validation_matches_clone_contract() {
        assert!(is_repository_branch_name_valid("feature/demo"));
        assert!(is_repository_branch_name_valid(&"é".repeat(255)));
        assert!(!is_repository_branch_name_valid(&"\u{1F680}".repeat(128)));
        assert!(!is_repository_branch_name_valid("../bad"));
        assert!(!is_repository_branch_name_valid("bad branch"));
    }

    #[tokio::test]
    async fn clone_process_terminates_when_job_is_canceled() {
        if !Path::new("/bin/sh").exists() {
            return;
        }
        let dir = tempdir().unwrap();
        let started_path = dir.path().join("started");
        let script_path = dir.path().join("fake-git.sh");
        fs::write(
            &script_path,
            r#"trap 'exit 130' TERM
printf 'clone started\n'
: > "$1"
while :; do :; done
"#,
        )
        .unwrap();
        let jobs = Arc::new(Mutex::new(HashMap::new()));
        let job_id = "job-cancel".to_string();
        jobs.lock().await.insert(
            job_id.clone(),
            json!({
                "jobId": job_id,
                "state": "running"
            }),
        );
        let run = tokio::spawn(run_clone_process(
            "/bin/sh",
            vec![
                script_path.to_string_lossy().to_string(),
                started_path.to_string_lossy().to_string(),
            ],
            dir.path().to_string_lossy().to_string(),
            jobs.clone(),
            job_id.clone(),
        ));
        for _ in 0..100 {
            if started_path.exists() {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
        assert!(started_path.exists());
        {
            let mut jobs = jobs.lock().await;
            jobs.get_mut(&job_id)
                .and_then(Value::as_object_mut)
                .unwrap()
                .insert("state".to_string(), json!("canceled"));
        }
        let output = tokio::time::timeout(Duration::from_secs(5), run)
            .await
            .expect("canceled clone should exit")
            .expect("join")
            .expect("clone output");
        assert_eq!(output.exit_code, 130);
        assert_eq!(output.stdout, "clone started");
    }
}
