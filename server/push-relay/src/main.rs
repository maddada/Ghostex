//! `ghostex-push-relay`: see `lib.rs` and `README.md`.

use std::io::Write as _;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ghostex_push_relay::apns::{Apple, Credentials, Target};
use ghostex_push_relay::limits::LimitConfig;
use ghostex_push_relay::{LogSink, Options, Relay};

struct Args {
    listen: SocketAddr,
    key: PathBuf,
    config: PathBuf,
    behind_proxy: bool,
    stand_in: Option<String>,
}

const USAGE: &str = "usage: ghostex-push-relay [--listen 127.0.0.1:58810] [--key /etc/ghostex-push-relay/AuthKey.p8] \
[--config /etc/ghostex-push-relay/apns.conf] [--behind-proxy] [--apple-stand-in http://127.0.0.1:<port>]";

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        listen: "127.0.0.1:58810".parse().expect("default address"),
        key: PathBuf::from("/etc/ghostex-push-relay/AuthKey.p8"),
        config: PathBuf::from("/etc/ghostex-push-relay/apns.conf"),
        behind_proxy: false,
        stand_in: None,
    };
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        let mut value = || {
            iter.next()
                .ok_or_else(|| format!("{arg} needs a value\n{USAGE}"))
        };
        match arg.as_str() {
            "--listen" => {
                args.listen = value()?
                    .parse()
                    .map_err(|_| format!("--listen: not an address\n{USAGE}"))?
            }
            "--key" => args.key = PathBuf::from(value()?),
            "--config" => args.config = PathBuf::from(value()?),
            "--behind-proxy" => args.behind_proxy = true,
            // Local tests only: a stand-in for Apple on loopback (refused for any other address).
            "--apple-stand-in" => args.stand_in = Some(value()?),
            "--help" | "-h" => return Err(USAGE.to_string()),
            other => return Err(format!("unknown argument {other}\n{USAGE}")),
        }
    }
    Ok(args)
}

#[tokio::main]
async fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    let log: LogSink = Arc::new(|line: &str| {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    });
    let target = match &args.stand_in {
        Some(origin) => match Target::loopback(origin) {
            Ok(target) => target,
            Err(message) => {
                eprintln!("--apple-stand-in: {message}");
                std::process::exit(2);
            }
        },
        None => Target::Apple,
    };
    let apple = Credentials::load(&args.key, &args.config)
        .and_then(|credentials| Apple::new(credentials, target));
    match &apple {
        Ok(apple) => log(&format!(
            "configured key_id={} target={}",
            apple.key_id(),
            match &args.stand_in {
                Some(_) => "stand-in",
                None => "apple",
            }
        )),
        Err(reason) => log(&format!(
            "not configured: {reason}; pushes are refused until the key is installed"
        )),
    }
    let relay = Relay::new(Options {
        behind_proxy: args.behind_proxy,
        apple,
        limits: LimitConfig::default(),
        log: log.clone(),
    });
    let pruner = relay.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tick.tick().await;
            pruner.prune();
        }
    });
    let listener = match tokio::net::TcpListener::bind(args.listen).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("cannot listen on {}: {error}", args.listen);
            std::process::exit(1);
        }
    };
    log(&format!("listening on {}", args.listen));
    let app = relay
        .router()
        .into_make_service_with_connect_info::<SocketAddr>();
    let shutdown = async {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler");
        tokio::select! {
            _ = term.recv() => {}
            _ = tokio::signal::ctrl_c() => {}
        }
    };
    if let Err(error) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
    {
        eprintln!("server error: {error}");
        std::process::exit(1);
    }
}
