use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;

use structopt::StructOpt;

use tokio_stream::{Stream, StreamExt, StreamMap};
use futures::SinkExt;

use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tonic::transport::Server;

use crate::proto::event::Event::{Join, Leave, Log, ServerShutdown};
use futures_core::core_reexport::borrow::BorrowMut;
use proto::broadcast_server::{Broadcast, BroadcastServer};
use proto::User;
use std::ops::Deref;
use std::task::{Context, Poll};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::oneshot::error::RecvError;

/// Piping Server in Rust
#[derive(StructOpt, Debug)]
#[structopt(name = "broadcast-server")]
#[structopt(rename_all = "kebab-case")]
struct Opt {
    /// HTTP port
    #[structopt(long, default_value = "20000")]
    port: u16,
    #[structopt(long)]
    /// Enable HTTPS
    enable_https: bool,
    /// HTTPS port
    #[structopt(long)]
    https_port: Option<u16>,
    /// Certification path
    #[structopt(long)]
    crt_path: Option<String>,
    /// Private key path
    #[structopt(long)]
    key_path: Option<String>,
}

pub mod proto {
    tonic::include_proto!("proto");
}

/// Shorthand for the transmit half of the message channel.
type Tx = mpsc::UnboundedSender<Result<proto::Event, tonic::Status>>;

#[derive(Debug)]
pub struct Service {
    state: Arc<Mutex<Shared>>,
}

impl Service {
    fn new(state: Arc<Mutex<Shared>>) -> Self {
        Service { state }
    }
}

#[derive(Debug)]
struct Shared {
    peers: HashMap<SocketAddr, Peer>,
}

impl Shared {
    fn new() -> Self {
        Shared {
            peers: HashMap::new(),
        }
    }

    async fn broadcast(&mut self, event: proto::Event) {
        // If we fail to send messages to any actor, we need to remove
        // it, but we can't do so while iterating.
        let mut to_remove = Vec::new();

        for (addr, peer) in self.peers.iter_mut() {
            let addr = *addr;
            let msg = format!(
                "broadcasting message to user_id: {:?} at addr: {:?}",
                peer.user.token, addr
            );
            tracing::info!("{}", msg);
            let event = event.clone();

            if let Err(e) = peer.stream.send(Ok(event)) {
                let msg = format!(
                    "Failed to broadcast message to client {:?}; error {}",
                    peer.user.token, e
                );
                tracing::warn!("{}", msg);

                to_remove.push(addr);
            }
        }

        // Remove disconnected clients from underlining state.
        for addr in to_remove {
            tracing::info!("removing client {}", addr);
            self.peers.remove(&addr);
        }
    }
}

#[derive(Debug)]
pub struct Peer {
    stream: Tx,
    user: User,
    receiver: oneshot::Receiver<SocketAddr>,
}

/// A handle to this actor, used by the server.
#[derive(Debug)]
pub struct ClientHandle {}

// impl Peer {
//     async fn new(state: Arc<Mutex<Shared>>, addr: SocketAddr, user: User) -> DropReceiver<Result<?, Status>> {
//         let (tx, rx) = mpsc::unbounded_channel::<Result<proto::Event, tonic::Status>>();
//         let (kill_switch_tx, kill_switch_rx) = oneshot::channel::<SocketAddr>();
//
//         let peer = Peer {
//             stream: tx,
//             receiver: kill_switch_rx,
//             user,
//         };
//         state
//             .lock()
//             .await
//             .peers
//             .insert(addr, peer);
//
//         let rx = DropReceiver::new(rx, Some((kill_switch_tx, addr)))
//     }
// }

#[derive(Debug)]
pub struct DropReceiver<T> {
    client_kill_switch: Option<(oneshot::Sender<SocketAddr>, SocketAddr)>,
    inner: mpsc::UnboundedReceiver<T>,
}

impl<T> DropReceiver<T> {
    /// Create a new `DropReceiver`.
    pub fn new(
        recv: UnboundedReceiver<T>,
        kill_switch_rx: Option<(oneshot::Sender<SocketAddr>, SocketAddr)>,
    ) -> Self {
        Self { inner: recv, client_kill_switch: kill_switch_rx }
    }

    /// Get back the inner `UnboundedReceiver`.
    // pub fn into_inner(self) -> UnboundedReceiver<T> {
    //     self.inner
    // }

    /// Closes the receiving half of a channel without dropping it.
    ///
    /// This prevents any further messages from being sent on the channel while
    /// still enabling the receiver to drain messages that are buffered.
    pub fn close(&mut self) {
        self.inner.close()
    }
}

impl<T> Stream for DropReceiver<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.poll_recv(cx)
    }
}

impl<T> AsRef<UnboundedReceiver<T>> for DropReceiver<T> {
    fn as_ref(&self) -> &UnboundedReceiver<T> {
        &self.inner
    }
}

impl<T> AsMut<UnboundedReceiver<T>> for DropReceiver<T> {
    fn as_mut(&mut self) -> &mut UnboundedReceiver<T> {
        &mut self.inner
    }
}

impl<T> Deref for DropReceiver<T> {
    type Target = mpsc::UnboundedReceiver<T>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> Drop for DropReceiver<T> {
    fn drop(&mut self) {
        tracing::info!("Stream dropped! Client disconnected!");
        if let Some((kill_switch_tx, addr)) = self.client_kill_switch.take() {
            tracing::info!("attempting to drop client addr {}", addr);
            if let Err(e) = kill_switch_tx.send(addr) {
                tracing::warn!(message = "failed to send oneshot, the receiver dropped", error = ?e);
            }
        }
    }
}

#[tonic::async_trait]
impl Broadcast for Service {
    type JoinStreamStream =
    Pin<Box<dyn Stream<Item=Result<proto::Event, tonic::Status>> + Send + Sync + 'static>>;

    /// create_stream:
    async fn join_stream(
        &self,
        request: tonic::Request<proto::JoinRequest>,
    ) -> Result<tonic::Response<Self::JoinStreamStream>, tonic::Status> {
        let peer_addr = request.remote_addr();
        tracing::info!("Got a request from {}", peer_addr.unwrap());

        let req = request.into_inner();

        match req.user {
            Some(user) => {
                tracing::info!(message = "join request stream", user_id = ?user.token, user_name = ?user.name);

                // let rx = Peer::new(self.state.clone(), peer_addr.unwrap(), user);

                let (tx, rx) = mpsc::unbounded_channel::<Result<proto::Event, tonic::Status>>();
                let (kill_switch_tx, kill_switch_rx) = oneshot::channel::<SocketAddr>();

                let peer = Peer {
                    stream: tx,
                    receiver: kill_switch_rx,
                    user,
                };
                self.state
                    .lock()
                    .await
                    .peers
                    .insert(peer_addr.unwrap(), peer);

                let rx = DropReceiver::new(rx, Some((kill_switch_tx, peer_addr.unwrap())));
                Ok(tonic::Response::new(Box::pin(rx)))
            }
            None => Err(tonic::Status::new(
                tonic::Code::InvalidArgument,
                "name is invalid",
            )),
        }
    }

    /// broadcast_message:
    async fn broadcast_event(
        &self,
        request: tonic::Request<proto::Event>,
    ) -> Result<tonic::Response<proto::MessageAck>, tonic::Status> {
        tracing::info!("broadcast event received");

        let event: proto::Event = request.into_inner();
        let state = self.state.clone();

        tokio::spawn(async move {
            let mut state = state.lock().await;
            state.broadcast(event).await;
        });
        Ok(tonic::Response::new(proto::MessageAck {
            status: "SENT".to_string(),
        }))
    }
}

pub fn signal_channel() -> (oneshot::Sender<()>, oneshot::Receiver<()>) {
    oneshot::channel()
}

pub async fn wait_for_signal(tx: oneshot::Sender<()>) {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("SIGINT received: shutting down");
    let _ = tx.send(());
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (signal_tx, signal_rx) = signal_channel();
    // tracing_subscriber::FmtSubscriber::builder()
    //     .with_max_level(tracing::Level::INFO)
    //     .init();

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
            EnvFilter::from_default_env().add_directive("broadcast_server=info".parse()?),
        )
        // Log events when `tracing` spans are created, entered, exited, or
        // closed. When Tokio's internal tracing support is enabled (as
        // described above), this can be used to track the lifecycle of spawned
        // tasks on the Tokio runtime.
        .with_span_events(FmtSpan::FULL)
        // Set this subscriber as the default, to collect all traces emitted by
        // the program.
        .init();

    let addr: SocketAddr = "[::1]:20000".parse().unwrap();
    // let addr: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 50051);

    // let color = Style::new().blue();
    // println!("\nChat BroadcastService gRPC Server ready at: {}", color.apply_to(addr)); // 4.

    // let state = Arc::new(Mutex::new(Vec::new()));
    let state = Arc::new(Mutex::new(Shared::new()));
    // let state = Arc::new(Vec::new());
    let svc = BroadcastServer::new(Service::new(state));

    tracing::info!("server running on {}", addr);

    let _ = tokio::spawn(wait_for_signal(signal_tx));

    Server::builder()
        .trace_fn(|_| tracing::info_span!("broadcast_server"))
        .add_service(svc)
        .serve_with_shutdown(addr, async {
            signal_rx.await.ok();
            tracing::info!("Graceful context shutdown");
        })
        .await?;

    Ok(())
}
