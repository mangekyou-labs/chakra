use anyhow::Result;

fn print_help() {
    println!(
        "chakra-api-server {}\n\n         Usage: chakra-api-server [--listen-addr <ADDR>]\n\n         Options:\n  --listen-addr <ADDR> Override api.listen_addr for this replica\n  \
         -h, --help           Show this help\n  \
         -V, --version        Show version",
        env!("CARGO_PKG_VERSION")
    );
}

fn parse_args() -> Result<Option<Option<String>>> {
    let mut args = std::env::args().skip(1);
    let mut listen_addr = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--listen-addr" => listen_addr = Some(args.next().expect("--listen-addr requires a value")),
            "-h" | "--help" => {
                print_help();
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("chakra-api-server {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            _ => anyhow::bail!("unknown argument: {arg}"),
        }
    }
    Ok(Some(listen_addr))
}

#[tokio::main]
async fn main() -> Result<()> {
    let Some(listen_addr) = parse_args()? else {
        return Ok(());
    };
    if let Some(addr) = listen_addr {
        std::env::set_var("LISTEN_ADDR", addr);
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "api_server=info,router_engine=info,dex_adapters=info".into()),
        )
        .init();
    api_server::run_server().await
}
