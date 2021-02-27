use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;

use structopt::StructOpt;

use futures::{Stream, StreamExt};

use tokio::sync::{mpsc, Mutex, oneshot, RwLock};
use tonic::transport::Server;

use proto::broadcast_server::{Broadcast, BroadcastServer};
use proto::User;
use crate::proto::event::Event::{Join, Log, Leave, ServerShutdown};
use std::task::{Context, Poll};
use std::ops::Deref;
use futures_core::core_reexport::borrow::BorrowMut;
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
// type Tx = mpsc::Sender<Result<proto::Event, tonic::Status>>;
type Tx = mpsc::UnboundedSender<Result<proto::Event, tonic::Status>>;

#[derive(Debug)]
pub struct Service {
    // state: Arc<Vec<Peer>>,
    // state: Arc<Mutex<Vec<Peer>>>,

    state: Arc<Mutex<Shared>>,
}

impl Service {
    fn new(state: Arc<Mutex<Shared>>) -> Self {
        Service { state }
    }
    // fn new(state: Arc<Mutex<Vec<Peer>>>) -> Self {
    //     Service { state }
    // }
}

#[derive(Debug)]
struct Shared {
    peers: HashMap<SocketAddr, Peer>
}

impl Shared {
    fn new() -> Self {
        Shared {
            peers: HashMap::new(),
        }
    }

    async fn broadcast(&mut self, event: proto::Event) {
        for peer in self.peers.iter_mut() {
            let msg = format!("broadcasting message to user_id {:?}", peer.1.user.token);
            tracing::info!("{}", msg);
            let event = event.clone();
            let _ = peer.1.stream.send(Ok(event));
        }
    }
}

#[derive(Debug, Clone)]
pub struct Peer {
    addr: SocketAddr,
    stream: Tx,
    user: User,
    receiver: oneshot::Receiver<SocketAddr>,
}

pub struct DropReceiver<T> {
    chan: Option<(oneshot::Sender<SocketAddr>, SocketAddr)>,
    inner: mpsc::UnboundedReceiver<T>,
}

impl<T> Stream for DropReceiver<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Pin::new(&mut self.inner).poll_next(cx)
        self.inner.poll_recv(cx)
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
        if let Some((chan, addr)) = self.chan.take() {
            tracing::info!("client addr {}", addr);
            if let Err(e) = chan.send(addr) {
                tracing::warn!(message = "failed to send oneshot, the receiver dropped", error = ?e);
            }
        }
        // std::mem::replace(&mut self.chan, oneshot::Sender<usize>);
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
        println!("Got a request from {:?}", peer_addr);

        let req = request.into_inner();

        match req.user {
            Some(user) => {
                let (tx, rx) = mpsc::unbounded_channel::<Result<proto::Event, tonic::Status>>();

                // let user: User = match event {
                //     Join(join) => {
                //         match join.user {
                //             Some(user) => {
                //                 tracing::info!(message = "user joined", user_id = ?user.token, user_name = ?user.name);
                //                 // tokio::spawn(async move {
                //                 //     if let Err(e) = tx.send(Ok(req)).await {
                //                 //         println!("error: {:?}", e);
                //                 //     }
                //                 // });
                //                 user
                //             },
                //             None => { User { token: "".to_string(), name: "".to_string() } }
                //         }
                //         // if let Some(user) = join.user {
                //         //     tracing::info!(message = "user joined", user_id = ?user.token, user_name = ?user.name);
                //         //     user
                //         // }
                //     }
                //     Leave(leave) => {
                //         match leave.user {
                //             Some(user) => {
                //                 tracing::info!(message = "user leaving", user_id = ?user.token, user_name = ?user.name);
                //                 user
                //             },
                //             None => { User { token: "".to_string(), name: "".to_string() } }
                //         }
                //         // if let Some(user) = leave.user {
                //         //     tracing::info!(message = "user leaving", user_id = ?user.token, user_name = ?user.name);
                //         //     user
                //         // }
                //     }
                //     Log(log) => {
                //         match log.user {
                //             Some(user) => {
                //                 tracing::info!(message = "user log", user_id = ?user.token, user_name = ?user.name);
                //                 user
                //             },
                //             None => { User { token: "".to_string(), name: "".to_string() } }
                //         }
                //         // if let Some(user) = log.user {
                //         //     tracing::info!(message = "user log", user_id = ?user.token, user_name = ?user.name);
                //         //     user
                //         // }
                //     }
                //     ServerShutdown(_) => {
                //         tracing::info!(message = "event shutdown");
                //         User { token: "".to_string(), name: "".to_string() }
                //     }
                // };

                tracing::info!(message = "join request stream", user_id = ?user.token, user_name = ?user.name);

                let (oneshot_tx, oneshot_rx) = oneshot::channel::<SocketAddr>();

                let peer = Peer { addr: peer_addr.unwrap(), stream: tx, receiver: oneshot_rx, user };
                self.state.lock().await.peers.insert(peer_addr.unwrap(), peer);

                let rx = DropReceiver {
                    inner: rx,
                    chan: Some((oneshot_tx, peer_addr.unwrap())),
                };

                Ok(tonic::Response::new(Box::pin(
                    // tokio_stream::wrappers::UnboundedReceiverStream::new(rx),
                    rx
                )))
            }
            // maybe: Some(ref event)
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

        // let mut state = self.state.lock().await;
        // let mut state = self.state.clone();
        // let state = Arc::clone(&self.state);

        // tokio::task::spawn_blocking(move || {
        //     tokio::spawn(async move {
        //         {
        //             let mut state = state.lock().await;
        //             state.broadcast(event).await;
        //         }
        //     });
        // });


        // let state = self.state.clone();
        // for peer in &state[..] {
        //     let event = event.clone();
        //     tokio::task::spawn_blocking(move || {
        //         tokio::spawn(async move {
        //             if let Err(e) = peer.stream.send(Ok(event)).await {
        //                 let msg = format!(
        //                     "Failed to broadcast message to client {:?}; error {}",
        //                     peer.user.token, e
        //                 );
        //                 tracing::error!("{}", msg);
        //                 // tracing::error!(message = "failed to send message", error = ?e);
        //             }
        //             let msg = format!("broadcasting message to user_id {:?}", peer.user.token);
        //             tracing::info!("{}", msg);
        //             // tracing::info!(message = "broadcasting message", user_session_id = ?conn.session_id);
        //         });
        //     });
        // }

        // let clone_state = Arc::clone(&self.state);
        // let mut lock_state = clone_state.lock().await;
        // for (mut key, mut peer) in lock_state.peers.iter_mut() {
        //     let event = event.clone();
        //     tokio::task::spawn_blocking(move || {
        //         tokio::spawn(async move {
        //             if let Err(e) = peer.stream.send(Ok(event)).await {
        //                 let msg = format!(
        //                     "Failed to broadcast message to client {:?}; error {}",
        //                     peer.user.token, e
        //                 );
        //                 tracing::error!("{}", msg);
        //                 // tracing::error!(message = "failed to send message", error = ?e);
        //             }
        //             let msg = format!("broadcasting message to user_id {:?}", peer.user.token);
        //             tracing::info!("{}", msg);
        //             // tracing::info!(message = "broadcasting message", user_session_id = ?conn.session_id);
        //         });
        //     });
        // }

        // for mut peer in self.state.lock().await.to_vec() {
        //     let event = event.clone();
        //     tokio::task::spawn_blocking(move || {
        //         tokio::spawn(async move {
        //             if let Err(e) = peer.stream.send(Ok(event)) {
        //                 let msg = format!(
        //                     "Failed to broadcast message to client {:?}; error {}",
        //                     peer.user.token, e
        //                 );
        //                 tracing::error!("{}", msg);
        //             }
        //             let msg = format!("broadcasting message to user_id {:?}", peer.user.token);
        //             tracing::info!("{}", msg);
        //         });
        //     });
        // }

        // for mut peer in self.state.lock().await.to_vec() {
        //     let event = event.clone();
        //     tokio::spawn(async move {
        //         if let Err(e) = peer.stream.send(Ok(event)) {
        //             let msg = format!(
        //                 "Failed to broadcast message to client {:?}; error {}",
        //                 peer.user.token, e
        //             );
        //             tracing::error!("{}", msg);
        //         }
        //         let msg = format!("broadcasting message to user_id {:?}", peer.user.token);
        //         tracing::info!("{}", msg);
        //     });
        // }

        let state = self.state.clone();
        tokio::spawn(async move {
            let mut state = state.lock().await;

            for (addr, peer) in state.peers.iter_mut() {
                let event = event.clone();

                if let Err(e) = peer.stream.send(Ok(event)) {
                    let msg = format!(
                        "Failed to broadcast message to client {:?}; error {}",
                        peer.user.token, e
                    );
                    tracing::error!("{}", msg);
                }
                let msg = format!("broadcasting message to user_id: {:?} at addr: {:?}", peer.user.token, addr);
                tracing::info!("{}", msg);


            }
        });
        Ok(tonic::Response::new(proto::MessageAck { status: "SENT".to_string() }))
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

    // let addr: SocketAddr = "[::1]:20000".parse().unwrap();
    let addr: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9090);

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
