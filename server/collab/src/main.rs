//! `ghostex-collab serve`: the Ghostex Collab sidecar gxserver starts once Collab is installed.
//! See lib.rs and serve.rs.

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("serve") => {}
        Some("--version") => {
            println!("ghostex-collab {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        _ => {
            eprintln!("ghostex-collab is started by Ghostex (gxserver) once Collab is installed.");
            std::process::exit(2);
        }
    }
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("[collab] couldn't start: {error}");
            std::process::exit(1);
        }
    };
    if let Err(error) = runtime.block_on(ghostex_collab::serve::serve()) {
        eprintln!("[collab] {error}");
        std::process::exit(1);
    }
    // Leave at once: the relay link and every guest connection end with the process.
    std::process::exit(0);
}
