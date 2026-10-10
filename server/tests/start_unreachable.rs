#![cfg(unix)]

use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use gxserver::{
    constants::{
        GXSERVER_DEV_LOCAL_API_PORT_ENV, GXSERVER_PRODUCT, GXSERVER_PROTOCOL_VERSION,
        GXSERVER_VERSION,
    },
    paths::get_gxserver_paths,
    protocol::{
        ListenerConfig, ListenersConfig, MigrationStatus, RuntimeMetadata, ServerHealthResponse,
    },
    runtime::{create_source_build_identity, write_runtime_metadata},
};

fn run_start(delay: Duration, healthy: bool) -> (std::process::Output, usize) {
    let home = tempfile::tempdir().unwrap();
    let paths = get_gxserver_paths(Some(home.path().to_path_buf()));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let identity = create_source_build_identity(GXSERVER_VERSION);
    let metadata = RuntimeMetadata {
        build_identity: identity.clone(),
        pid: std::process::id(),
        port,
        protocol_version: GXSERVER_PROTOCOL_VERSION,
        server_id: "existing-daemon".into(),
        started_at: "2026-10-06T00:00:00Z".into(),
        version: GXSERVER_VERSION.into(),
    };
    write_runtime_metadata(&paths, &metadata).unwrap();
    let health = ServerHealthResponse {
        ok: true,
        product: GXSERVER_PRODUCT.into(),
        protocol_version: GXSERVER_PROTOCOL_VERSION,
        version: GXSERVER_VERSION.into(),
        build_identity: identity,
        capabilities: vec![],
        listeners: ListenersConfig {
            local: ListenerConfig::local_with_port(port),
            remote: ListenerConfig::remote_default(),
        },
        migration: MigrationStatus {
            applied_migrations: vec![],
            current_version: 0,
            state_db_file: "test".into(),
            state_imports: None,
        },
        pid: metadata.pid,
        portless: gxserver::portless::unavailable_portless_status_payload(),
        port,
        server_id: metadata.server_id.clone(),
        started_at: metadata.started_at.clone(),
        tools: vec![],
        launch_context: None,
    };
    let body = serde_json::to_string(&health).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let requests = Arc::new(AtomicUsize::new(0));
    let stop_server = stop.clone();
    let request_count = requests.clone();
    let server = thread::spawn(move || {
        let mut workers = vec![];
        while !stop_server.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let body = body.clone();
                    request_count.fetch_add(1, Ordering::SeqCst);
                    workers.push(thread::spawn(move || {
                        stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                        let _ = stream.read(&mut [0; 4096]);
                        thread::sleep(delay);
                        let code = if healthy { "200 OK" } else { "503 Service Unavailable" };
                        let _ = write!(stream, "HTTP/1.1 {code}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                    }));
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(error) => panic!("accept health request: {error}"),
            }
        }
        for worker in workers {
            worker.join().unwrap();
        }
    });
    let output = Command::new(env!("CARGO_BIN_EXE_gxserver"))
        .args(["start", "--json"])
        .env("HOME", home.path())
        .env("GHOSTEX_HOME", home.path().join(".ghostex"))
        .env(GXSERVER_DEV_LOCAL_API_PORT_ENV, port.to_string())
        .env("GHOSTEX_AGENT_SKILLS_REMOTE", "off")
        .output()
        .unwrap();
    stop.store(true, Ordering::SeqCst);
    server.join().unwrap();
    let preserved: RuntimeMetadata =
        serde_json::from_slice(&std::fs::read(&paths.runtime_metadata_file).unwrap()).unwrap();
    assert_eq!(preserved.server_id, "existing-daemon");
    (output, requests.load(Ordering::SeqCst))
}

#[test]
fn live_unreachable_daemon_refuses_a_second_start() {
    let (output, requests) = run_start(Duration::ZERO, false);
    assert!(!output.status.success());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(
        error.contains(&format!("pid {} is running", std::process::id())),
        "{error}"
    );
    assert!(error.contains("within 5000 ms"), "{error}");
    assert!(requests > 1);
}

#[test]
fn health_slower_than_initial_timeout_reuses_the_live_daemon() {
    let (output, requests) = run_start(Duration::from_millis(950), true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let status: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["state"], "running");
    assert_eq!(status["health"]["pid"], std::process::id());
    assert!(requests >= 2);
}
