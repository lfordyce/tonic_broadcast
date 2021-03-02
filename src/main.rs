pub mod cmd;
use clap::Clap;

/// runs the gRPC server
#[derive(Clap)]
pub struct ServerOpts {
    /// Sets the server address
    #[clap(short, long, default_value = "[::1]:20000")]
    pub server_listen_addr: String,
}

/// runs the gRPC client
#[derive(Clap, Debug)]
pub struct ClientOpts {
    #[clap(short, long, default_value = "http://[::1]:20000")]
    pub server_addr: String,
}

#[derive(Clap)]
enum SubCommand {
    #[clap(name = "server")]
    Server(ServerOpts),
    #[clap(name = "client")]
    Client(ClientOpts),
}

#[derive(Clap)]
#[clap(version = "1.0", author = "fordyce")]
struct Opts {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opts: Opts = Opts::parse();

    use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};
    // Configure a `tracing` subscriber that logs traces emitted by the chat
    // server.
    tracing_subscriber::fmt()
        // Filter what traces are displayed based on the RUST_LOG environment
        // variable.
        //
        // Traces emitted by the example code will always be displayed. You
        // can set `RUST_LOG=tokio=trace` to enable additional traces emitted by
        // Tokio itself.
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("tonic_broadcast=info".parse()?),
        )
        // Log events when `tracing` spans are created, entered, exited, or
        // closed. When Tokio's internal tracing support is enabled (as
        // described above), this can be used to track the lifecycle of spawned
        // tasks on the Tokio runtime.
        .with_span_events(FmtSpan::FULL)
        // Set this subscriber as the default, to collect all traces emitted by
        // the program.
        .init();

    match opts.subcmd {
        SubCommand::Server(s) => {
            // println!("Start the server on: {:?}", s.server_listen_addr);
            cmd::server::start_server(s).await?;
        }
        SubCommand::Client(c) => {
            tracing::info!("Client started connected to: {}", c.server_addr);
            cmd::client::client_run(c).await?;
        }
    }
    Ok(())
}
