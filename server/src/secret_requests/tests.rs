use std::time::Duration;

use super::store::{self, NewSecretRequest, SecretAnswer, SecretRequestError, SecretRequestStatus};
use crate::paths::get_gxserver_paths;

/// The registry is process-wide and tests run in parallel, so each test uses its own session.
fn request(session: &str) -> String {
    store::create(NewSecretRequest {
        project_id: "Ptest".to_string(),
        session_id: session.to_string(),
        label: "Example API token".to_string(),
        reason: "The example CLI needs it to deploy.".to_string(),
        placeholder: Some("Paste the token".to_string()),
        accepts_reference: false,
        timeout: Duration::from_secs(60),
    })
    .unwrap()
}

#[test]
fn a_saved_secret_reaches_only_an_owner_only_file() {
    let temp = tempfile::tempdir().unwrap();
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let id = request("save");
    let views = store::list_for_session("Ptest", "save");
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].status, SecretRequestStatus::Pending);
    let shown = serde_json::to_string(&views).unwrap();
    assert!(shown.contains("Example API token"));

    store::answer(
        &paths,
        "Ptest",
        "save",
        &id,
        SecretAnswer::Save("not-a-real-token-123".to_string()),
    )
    .unwrap();
    // What clients see never carries the value or the file.
    let shown = serde_json::to_string(&store::list_for_session("Ptest", "save")).unwrap();
    assert!(!shown.contains("not-a-real-token-123"));
    assert!(!shown.contains("secret-requests"));

    let outcome = store::wait(&id, Duration::from_millis(50)).unwrap();
    assert_eq!(outcome.status, SecretRequestStatus::Saved);
    let file = outcome.file.expect("a saved request names its file");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "not-a-real-token-123"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let dir_mode = std::fs::metadata(file.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dir_mode, 0o700);
    }
    // Handed over once.
    let again = store::wait(&id, Duration::from_millis(10)).unwrap();
    assert_eq!(again.file, None);
    // A second answer is refused.
    assert_eq!(
        store::answer(&paths, "Ptest", "save", &id, SecretAnswer::Decline),
        Err(SecretRequestError::AlreadyAnswered)
    );
}

#[test]
fn an_answer_must_come_from_the_asking_session() {
    let temp = tempfile::tempdir().unwrap();
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let id = request("owner");
    assert_eq!(
        store::answer(&paths, "Ptest", "other", &id, SecretAnswer::Decline),
        Err(SecretRequestError::NotFound)
    );
    store::answer(&paths, "Ptest", "owner", &id, SecretAnswer::Decline).unwrap();
    let outcome = store::wait(&id, Duration::from_millis(10)).unwrap();
    assert_eq!(outcome.status, SecretRequestStatus::Declined);
    assert_eq!(outcome.file, None);
}

#[test]
fn an_empty_answer_is_refused_and_the_card_stays_open() {
    let temp = tempfile::tempdir().unwrap();
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let id = request("empty");
    assert!(matches!(
        store::answer(
            &paths,
            "Ptest",
            "empty",
            &id,
            SecretAnswer::Save("   ".to_string())
        ),
        Err(SecretRequestError::Invalid(_))
    ));
    assert_eq!(
        store::list_for_session("Ptest", "empty")[0].status,
        SecretRequestStatus::Pending
    );
}

#[test]
fn a_waiter_wakes_when_the_user_answers() {
    let temp = tempfile::tempdir().unwrap();
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let id = request("wake");
    let waiting_id = id.clone();
    let waiter = std::thread::spawn(move || store::wait(&waiting_id, Duration::from_secs(10)));
    std::thread::sleep(Duration::from_millis(100));
    store::answer(&paths, "Ptest", "wake", &id, SecretAnswer::Decline).unwrap();
    let outcome = waiter.join().unwrap().unwrap();
    assert_eq!(outcome.status, SecretRequestStatus::Declined);
}

#[test]
fn a_request_nobody_waits_for_closes_and_refuses_answers() {
    let temp = tempfile::tempdir().unwrap();
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let id = request("gone");
    store::backdate_for_tests(&id, store::HEARTBEAT_GRACE + Duration::from_secs(1));
    assert_eq!(
        store::list_for_session("Ptest", "gone")[0].status,
        SecretRequestStatus::Cancelled
    );
    assert_eq!(
        store::answer(
            &paths,
            "Ptest",
            "gone",
            &id,
            SecretAnswer::Save("not-a-real-token".to_string())
        ),
        Err(SecretRequestError::Ended)
    );
    assert!(!store::files_dir(&paths).join(&id).exists());
}

#[test]
fn cancelling_closes_the_card() {
    let id = request("cancel");
    assert!(store::cancel(&id));
    assert!(!store::cancel(&id));
    assert_eq!(
        store::wait(&id, Duration::from_millis(10)).unwrap().status,
        SecretRequestStatus::Cancelled
    );
}

#[test]
fn a_session_cannot_pile_up_cards() {
    for _ in 0..4 {
        request("pile");
    }
    assert!(matches!(
        store::create(NewSecretRequest {
            project_id: "Ptest".to_string(),
            session_id: "pile".to_string(),
            label: "One more".to_string(),
            reason: "Too many".to_string(),
            placeholder: None,
            accepts_reference: false,
            timeout: Duration::from_secs(60),
        }),
        Err(SecretRequestError::TooMany)
    ));
}

#[test]
fn old_files_are_swept_and_others_left_alone() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let old = dir.join("0123456789abcdef0123456789abcdef");
    std::fs::write(&old, "x").unwrap();
    let unrelated = dir.join("notes.txt");
    std::fs::write(&unrelated, "x").unwrap();
    let past = std::time::SystemTime::now() - Duration::from_secs(25 * 60 * 60);
    std::fs::File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_modified(past)
        .unwrap();
    assert_eq!(store::sweep_expired_files(dir), 1);
    assert!(!old.exists());
    assert!(unrelated.exists());
}
