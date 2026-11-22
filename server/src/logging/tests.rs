use super::*;
use crate::paths::get_gxserver_paths;
use std::sync::{mpsc, Arc, Barrier};

#[test]
fn warn_log_redacts_private_values() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let logger = GxserverLogger::new(paths.clone());
    logger
        .log(GxserverLogInput {
            level: LogLevel::Warn,
            event: "test".to_string(),
            server_id: Some("S1a".to_string()),
            request_id: Some("request-1".to_string()),
            client: None,
            duration_ms: None,
            error: Some("failed /Users/alice/project token=secret".to_string()),
            details: Some(json!({
                "path": "/Users/alice/project",
                "url": "https://example.com/private?token=secret",
                "command": "cat ~/.ssh/id_rsa",
                "args": ["--token", "secret"],
                "projectName": "Private Alpha",
                "stdout": "raw command output",
                "stderr": "raw error output",
                "projectId": "P1abc"
            })),
        })
        .expect("log");
    let text = fs::read_to_string(paths.log_file).expect("read log");
    assert!(!text.contains("/Users/alice"));
    assert!(!text.contains("id_rsa"));
    assert!(!text.contains("token=secret"));
    assert!(!text.contains("Private Alpha"));
    assert!(!text.contains("raw command output"));
    assert!(!text.contains("raw error output"));
    assert!(text.contains("P1abc"));
}

#[test]
fn persistent_log_boundary_redacts_prompts_env_urls_titles_commands_and_output() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let logger = GxserverLogger::new(paths.clone());
    logger
        .log(GxserverLogInput {
            level: LogLevel::Warn,
            event: "privacy.boundary".to_string(),
            server_id: Some("S7k".to_string()),
            request_id: Some("request-privacy".to_string()),
            client: None,
            duration_ms: None,
            error: Some(
                "failed while running /Users/person/dev/private-project with HTTPS://Example.test/private?token=SECRET"
                    .to_string(),
            ),
            details: Some(json!({
                "args": ["commit", "-m", "private command subject"],
                "authToken": "SECRET",
                "commandText": "git commit -m 'private command subject'",
                "env": {
                    "CUSTOMER_NAME": "Acme Private",
                    "GHOSTEX_TOKEN": "SECRET"
                },
                "environment": "PATH=/Users/person/dev/private-project TOKEN=SECRET",
                "projectName": "Private Project",
                "prompt": "Summarize private customer incident",
                "prompts": ["private prompt one", "private prompt two"],
                "rawUrl": "HTTPS://Example.test/private?token=SECRET",
                "sessionName": "Customer Debug Session",
                "stderr": "private stderr output",
                "stdout": "private stdout output",
                "terminalTitle": "Private Terminal Title",
                "url": "HTTPS://Example.test/private?token=SECRET",
                "workspaceRoot": "/Users/person/dev/private-project"
            })),
        })
        .expect("log");

    let text = fs::read_to_string(paths.log_file).expect("read log");
    for forbidden in [
        "/Users/person",
        "Acme Private",
        "Customer Debug Session",
        "Example.test/private",
        "HTTPS://",
        "Private Project",
        "Private Terminal Title",
        "Summarize private customer incident",
        "git commit",
        "private command subject",
        "private prompt one",
        "private stderr output",
        "private stdout output",
        "SECRET",
        "TOKEN=SECRET",
        "private-project",
    ] {
        assert!(
            !text.contains(forbidden),
            "persistent log leaked {forbidden}: {text}"
        );
    }
    assert!(text.contains("[redacted]"));
    assert!(text.contains("[redacted:path]"));
    assert!(text.contains("[redacted:url]"));
    assert!(text.contains("[redacted:secret]"));
}

#[test]
fn routine_logs_are_gated_by_debugging_mode() {
    let disabled_temp = tempfile::tempdir().expect("disabled tempdir");
    let disabled_paths = get_gxserver_paths(Some(disabled_temp.path().to_path_buf()));
    let disabled_logger = test_logger(disabled_paths.clone());
    disabled_logger
        .log_routine(
            DiagnosticLogScenario::ServerLifecycle,
            GxserverLogInput {
                level: LogLevel::Info,
                event: "routine.info".to_string(),
                server_id: None,
                request_id: None,
                client: None,
                duration_ms: None,
                error: None,
                details: None,
            },
        )
        .expect("info log");
    disabled_logger
        .log_routine(
            DiagnosticLogScenario::ServerLifecycle,
            GxserverLogInput {
                level: LogLevel::Debug,
                event: "routine.debug".to_string(),
                server_id: None,
                request_id: None,
                client: None,
                duration_ms: None,
                error: None,
                details: None,
            },
        )
        .expect("debug log");
    disabled_logger
        .log_routine(
            DiagnosticLogScenario::ServerLifecycle,
            GxserverLogInput {
                level: LogLevel::Info,
                event: "routine.health".to_string(),
                server_id: None,
                request_id: None,
                client: None,
                duration_ms: None,
                error: None,
                details: Some(json!({ "errorCount": 0, "status": "ok" })),
            },
        )
        .expect("zero-error routine log");
    assert!(!disabled_paths.log_file.exists());
    disabled_logger
        .log(GxserverLogInput {
            level: LogLevel::Info,
            event: "important.failure".to_string(),
            server_id: None,
            request_id: None,
            client: None,
            duration_ms: None,
            error: Some("important failure".to_string()),
            details: None,
        })
        .expect("important log");
    let important_text = fs::read_to_string(&disabled_paths.log_file).expect("read important log");
    assert!(important_text.contains("important.failure"));

    let enabled_temp = tempfile::tempdir().expect("enabled tempdir");
    let enabled_paths = get_gxserver_paths(Some(enabled_temp.path().to_path_buf()));
    let enabled_logger = test_logger_with_debugging_mode(enabled_paths.clone(), true);
    enabled_logger
        .log_routine(
            DiagnosticLogScenario::ServerLifecycle,
            GxserverLogInput {
                level: LogLevel::Info,
                event: "routine.info".to_string(),
                server_id: None,
                request_id: None,
                client: None,
                duration_ms: None,
                error: None,
                details: None,
            },
        )
        .expect("info log enabled");
    enabled_logger
        .log_routine(
            DiagnosticLogScenario::ServerLifecycle,
            GxserverLogInput {
                level: LogLevel::Debug,
                event: "routine.debug".to_string(),
                server_id: None,
                request_id: None,
                client: None,
                duration_ms: None,
                error: None,
                details: None,
            },
        )
        .expect("debug log enabled");
    enabled_logger
        .log(GxserverLogInput {
            level: LogLevel::Info,
            event: "routine.unscoped".to_string(),
            server_id: None,
            request_id: None,
            client: None,
            duration_ms: None,
            error: None,
            details: None,
        })
        .expect("unscoped routine log ignored");
    let text = fs::read_to_string(enabled_paths.log_file).expect("read enabled logs");
    assert!(text.contains("\"routine.info\""));
    assert!(text.contains("\"routine.debug\""));
    assert!(!text.contains("routine.unscoped"));
}

#[test]
fn log_query_filters_malformed_identities_timestamps_limit_and_order() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    fs::create_dir_all(&paths.logs_dir).expect("logs dir");
    fs::write(
        &paths.log_file,
        [
            log_line(json!({
                "client": "cli",
                "event": "agent.detected",
                "level": "info",
                "projectId": "P3a91",
                "serverId": "S7k",
                "sessionId": "G8v20",
                "ts": "2026-05-30T10:00:00.000Z"
            })),
            "not-json".to_string(),
            log_line(json!({
                "client": "api",
                "event": "agent.activity.working",
                "level": "debug",
                "projectId": "P3a91",
                "serverId": "S7k",
                "sessionId": "G8v20",
                "ts": "2026-05-30T10:01:00.000Z"
            })),
            log_line(json!({
                "client": "api",
                "event": "zmx.kill.failed",
                "level": "error",
                "projectId": "P4b12",
                "serverId": "S7k",
                "sessionId": "G9v21",
                "ts": "2026-05-30T10:02:00.000Z"
            })),
        ]
        .join("\n")
            + "\n",
    )
    .expect("write log");

    let params = params(json!({
        "eventPrefix": "agent.",
        "level": ["debug", "info"],
        "limit": 1,
        "order": "desc",
        "projectId": "P3a91",
        "since": "2026-05-30T09:59:00.000Z",
        "until": "2026-05-30T10:01:30.000Z"
    }));
    let result = query_gxserver_logs(&paths, &params).expect("query logs");

    assert_eq!(result["malformedLineCount"], json!(1));
    assert_eq!(result["malformedLineCountIsExact"], json!(true));
    assert_eq!(result["totalMatched"], json!(2));
    assert_eq!(result["totalMatchedIsExact"], json!(true));
    assert_eq!(result["truncated"], json!(false));
    let entries = result["entries"].as_array().expect("entries");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["event"], json!("agent.activity.working"));
}

#[test]
fn large_descending_log_query_reads_bounded_tail_window() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    fs::create_dir_all(&paths.logs_dir).expect("logs dir");
    let mut lines = Vec::new();
    let mut byte_length = 0_u64;
    let mut line_count = 0_usize;
    while byte_length <= LOG_QUERY_FULL_SCAN_MAX_BYTES + 1024 * 1024 {
        let line = log_line(json!({
            "event": format!("tail.{line_count}"),
            "level": "info",
            "message": "x".repeat(768),
            "ts": DateTime::from_timestamp(1_780_138_800 + line_count as i64, 0)
                .expect("timestamp")
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        }));
        byte_length += line.len() as u64 + 1;
        lines.push(line);
        line_count += 1;
    }
    fs::write(&paths.log_file, format!("{}\n", lines.join("\n"))).expect("write log");

    let result = query_gxserver_logs(
        &paths,
        &params(json!({
            "eventPrefix": "tail.",
            "limit": 3,
            "order": "desc"
        })),
    )
    .expect("query logs");

    let events = result["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|entry| entry["event"].as_str().unwrap_or_default().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        events,
        vec![
            format!("tail.{}", line_count - 1),
            format!("tail.{}", line_count - 2),
            format!("tail.{}", line_count - 3),
        ]
    );
    assert_eq!(result["truncated"], json!(true));
    assert_eq!(result["truncatedReason"], json!("fileWindowExceeded"));
    assert_eq!(result["totalMatchedIsExact"], json!(false));
    assert_eq!(result["malformedLineCountIsExact"], json!(false));
    assert!(
        result["scannedBytes"].as_u64().unwrap() < result["logFileSizeBytes"].as_u64().unwrap()
    );
    assert!(result["scannedLineCount"].as_u64().unwrap() < line_count as u64);
}

#[test]
fn log_query_rejects_invalid_limit_and_conflicting_order() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));

    let limit_error =
        query_gxserver_logs(&paths, &params(json!({ "limit": 0 }))).expect_err("limit error");
    assert!(
        matches!(limit_error, LogQueryError::Input(message) if message == "limit must be an integer from 1 to 5000.")
    );

    let order_error =
        query_gxserver_logs(&paths, &params(json!({ "order": "asc", "reverse": true })))
            .expect_err("order error");
    assert!(
        matches!(order_error, LogQueryError::Input(message) if message == "order and reverse specify conflicting log order.")
    );
}

#[test]
fn log_retention_keeps_active_split_file_and_deletes_older_rotations() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    fs::create_dir_all(&paths.logs_dir).expect("logs dir");
    fs::write(&paths.log_file, "old-active\nnew-active-1\nnew-active-2\n").expect("active");
    fs::write(
        rotated_log_file(&paths.log_file, 1),
        "old-rotated\nnew-rotated\n",
    )
    .expect("rotated 1");
    fs::write(rotated_log_file(&paths.log_file, 2), "older-rotated\n").expect("rotated 2");

    prune_gxserver_log_lines(&paths, 2).expect("prune");

    assert_eq!(
        fs::read_to_string(&paths.log_file).expect("read active"),
        "new-active-1\nnew-active-2\n"
    );
    assert!(!rotated_log_file(&paths.log_file, 1).exists());
    assert!(!rotated_log_file(&paths.log_file, 2).exists());
}

#[test]
fn retention_prune_blocks_logger_appends_until_rewrite_finishes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    fs::create_dir_all(&paths.logs_dir).expect("logs dir");
    fs::write(&paths.log_file, "old-0\nold-1\nold-2\nold-3\n").expect("active");
    let logger = Arc::new(test_logger(paths.clone()));
    let (append_started_tx, append_started_rx) = mpsc::channel();
    let (append_done_tx, append_done_rx) = mpsc::channel();
    let (append_handle_tx, append_handle_rx) = mpsc::channel();
    let logger_for_append = Arc::clone(&logger);

    prune_gxserver_log_lines_with_before_rewrite(&paths, 2, move || {
        let append_handle = thread::spawn(move || {
            append_started_tx.send(()).expect("append started");
            logger_for_append
                .log(GxserverLogInput {
                    level: LogLevel::Warn,
                    event: "retention.append.during-prune".to_string(),
                    server_id: None,
                    request_id: None,
                    client: None,
                    duration_ms: None,
                    error: None,
                    details: None,
                })
                .expect("append log");
            let _ = append_done_tx.send(());
        });
        append_handle_tx.send(append_handle).expect("append handle");
        append_started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("append attempted");
        match append_done_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(()) => panic!("logger append completed while retention held the write lock"),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(error) => panic!("append done channel failed: {error}"),
        }
    })
    .expect("prune");

    append_handle_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("append handle")
        .join()
        .expect("append thread");
    let text = fs::read_to_string(&paths.log_file).expect("read active");
    assert!(text.contains("\"retention.append.during-prune\""));
}

#[test]
fn concurrent_retention_prune_keeps_logger_appends() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    fs::create_dir_all(&paths.logs_dir).expect("logs dir");
    let old_lines = (0..200)
        .map(|index| {
            log_line(json!({
                "event": format!("old.{index}"),
                "level": "warn"
            }))
        })
        .collect::<Vec<_>>();
    fs::write(&paths.log_file, format!("{}\n", old_lines.join("\n"))).expect("active");
    let append_count = 32;
    let prune_count = 8;
    let max_lines = append_count + 8;
    let logger = Arc::new(test_logger(paths.clone()));
    let start = Arc::new(Barrier::new(append_count + prune_count + 1));
    let mut handles = Vec::new();

    for index in 0..append_count {
        let logger = Arc::clone(&logger);
        let start = Arc::clone(&start);
        handles.push(thread::spawn(move || {
            start.wait();
            logger
                .log(GxserverLogInput {
                    level: LogLevel::Warn,
                    event: format!("retention.append.{index}"),
                    server_id: None,
                    request_id: None,
                    client: None,
                    duration_ms: None,
                    error: None,
                    details: None,
                })
                .expect("append log");
        }));
    }
    for _ in 0..prune_count {
        let paths = paths.clone();
        let start = Arc::clone(&start);
        handles.push(thread::spawn(move || {
            start.wait();
            prune_gxserver_log_lines(&paths, max_lines).expect("prune");
        }));
    }

    start.wait();
    for handle in handles {
        handle.join().expect("worker thread");
    }
    let text = fs::read_to_string(&paths.log_file).expect("read active");
    for index in 0..append_count {
        assert!(
            text.contains(&format!("\"retention.append.{index}\"")),
            "missing append {index}"
        );
    }
}

#[test]
fn logger_startup_schedules_line_retention() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    fs::create_dir_all(&paths.logs_dir).expect("logs dir");
    fs::write(&paths.log_file, "old\nnew-1\nnew-2\n").expect("active");

    let _logger = GxserverLogger::new_with_retention(
        paths.clone(),
        LogRetentionOptions {
            delay_ms: 1,
            max_lines: 2,
        },
    );
    wait_for_log_file_text(&paths.log_file, "new-1\nnew-2\n");
}

fn test_logger(paths: GxserverPaths) -> GxserverLogger {
    test_logger_with_debugging_mode(paths, false)
}

fn test_logger_with_debugging_mode(paths: GxserverPaths, debugging_mode: bool) -> GxserverLogger {
    GxserverLogger {
        paths,
        debugging_mode_cache: Mutex::new(DebuggingModeCache {
            checked_at: Instant::now(),
            debugging_mode,
            enabled_scenarios: debugging_mode
                .then(|| DiagnosticLogScenario::ServerLifecycle.id().to_string())
                .into_iter()
                .collect(),
        }),
    }
}

fn wait_for_log_file_text(log_file: &Path, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let current = fs::read_to_string(log_file).unwrap_or_default();
        if current == expected {
            return;
        }
        if Instant::now() >= deadline {
            assert_eq!(current, expected);
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn log_line(value: Value) -> String {
    serde_json::to_string(&value).expect("json line")
}

fn params(value: Value) -> Map<String, Value> {
    value.as_object().expect("params object").clone()
}
