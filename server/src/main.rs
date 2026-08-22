#[tokio::main]
async fn main() {
    if let Err(error) = gxserver::cli::run_from_env().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
