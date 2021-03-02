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
#[derive(Clap)]
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
    match opts.subcmd {
        SubCommand::Server(s) => {
            println!("Start the server on: {:?}", s.server_listen_addr);
            cmd::server::start_server(s).await?;
        }
        SubCommand::Client(c) => {
            println!("Client started connected to: '{:?}'", c.server_addr);
            cmd::client::client_run(c).await?;
        }
    }
    Ok(())
}
