use anyhow::Result;

fn print_help() {
    println!(
        "chakra-market-data-worker {}\n\n         Usage: chakra-market-data-worker\n\n         Reads all configuration from environment variables.\n\n         Options:\n  -h, --help      Show this help\n  \
         -V, --version   Show version",
        env!("CARGO_PKG_VERSION")
    );
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            "-V" | "--version" => {
                println!("chakra-market-data-worker {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            _ => anyhow::bail!("unknown argument: {arg}"),
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "market_data_worker=info,dex_adapters=info".into()),
        )
        .init();
    market_data_worker::worker::run(market_data_worker::worker::WorkerConfig::from_env()?).await
}
