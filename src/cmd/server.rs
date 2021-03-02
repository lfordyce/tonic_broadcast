use std::collections::HashMap;
use std::net::SocketAddr;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use tokio::io;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_stream::Stream;
use tonic::transport::Server;

use proto::broadcast_server::{Broadcast, BroadcastServer};
use proto::event::Event::{Join, Leave, Log, ServerShutdown};
use proto::User;

use crate::ServerOpts;

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

#[derive(Default, Debug)]
struct Data {
    peers: Arc<Mutex<HashMap<SocketAddr, Peer>>>,
}

#[derive(Debug)]
struct Shared {
    dispatch: mpsc::UnboundedSender<ToServer>,
}

impl Shared {
    fn new() -> Self {
        let (send, recv) = mpsc::unbounded_channel::<ToServer>();

        tokio::spawn(async move {
            let res = server_loop(recv).await;
            match res {
                Ok(()) => {}
                Err(err) => {
                    eprintln!("Oops {}.", err);
                }
            }
        });

        Shared { dispatch: send }
    }
}

/// The message type used when a client actor sends messages to the main loop.
pub enum ToServer {
    NewClient(SocketAddr, Peer, oneshot::Receiver<SocketAddr>),
    Event(SocketAddr, proto::Event),
    FatalError(io::Error),
}

async fn listen_for_drop(state: Arc<Mutex<HashMap<SocketAddr, Peer>>>, recv: oneshot::Receiver<SocketAddr>) {
    match recv.await {
        Ok(addr) => {
            tracing::info!("drop request received - removing client: {}", addr);
            state.lock().await.remove(&addr);
        }
        Err(_) => tracing::warn!("the client oneshot sender dropped..."),
    }
}

async fn server_loop(mut recv: mpsc::UnboundedReceiver<ToServer>) -> Result<(), io::Error> {
    let data = Data::default();
    while let Some(msg) = recv.recv().await {
        match msg {
            ToServer::NewClient(addr, peer, kill_switch) => {
                // spawn task to listen for client drops.
                tokio::spawn(listen_for_drop(data.peers.clone(), kill_switch));
                data.peers.lock().await.insert(addr, peer);
            }
            ToServer::Event(from_addr, event) => {
                let mut state = data.peers.lock().await;

                for (addr, peer) in state.iter_mut() {
                    let addr = *addr;

                    let msg = format!("broadcasting message to user_id: {:?} at addr: {:?}", peer.user.token, addr);
                    tracing::info!("{}", msg);

                    // Don't send it to the client who sent it to us.
                    if addr == from_addr { continue; }

                    let event = event.clone();

                    if let Err(e) = peer.stream.send(Ok(event)) {
                        let msg = format!(
                            "Failed to broadcast message to client {:?}; error {}",
                            peer.user.token, e
                        );
                        tracing::warn!("{}", msg);
                    }
                }
            }
            ToServer::FatalError(err) => return Err(err),
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct Peer {
    stream: Tx,
    user: User,
}

/// A handle to this actor, used by the server.
#[derive(Debug)]
pub struct ClientHandle {}


#[derive(Debug)]
pub struct DropReceiver<T> {
    client_kill_switch: Option<(oneshot::Sender<SocketAddr>, SocketAddr)>,
    inner: mpsc::UnboundedReceiver<T>,
}

impl<T> DropReceiver<T> {
    /// Create a new `DropReceiver`.
    pub fn new(
        recv: mpsc::UnboundedReceiver<T>,
        kill_switch_rx: Option<(oneshot::Sender<SocketAddr>, SocketAddr)>,
    ) -> Self {
        Self {
            inner: recv,
            client_kill_switch: kill_switch_rx,
        }
    }

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

impl<T> AsRef<mpsc::UnboundedReceiver<T>> for DropReceiver<T> {
    fn as_ref(&self) -> &mpsc::UnboundedReceiver<T> {
        &self.inner
    }
}

impl<T> AsMut<mpsc::UnboundedReceiver<T>> for DropReceiver<T> {
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
        if let Some((kill_switch_tx, addr)) = self.client_kill_switch.take() {
            tracing::info!("Stream dropped: client disconnect detected dispatching address to remove from server state: {}", addr);
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

                let (tx, rx) = mpsc::unbounded_channel::<Result<proto::Event, tonic::Status>>();
                let (kill_switch_tx, kill_switch_rx) = oneshot::channel::<SocketAddr>();

                let peer = Peer {
                    stream: tx,
                    user,
                };

                match self.state.lock().await.dispatch.send(ToServer::NewClient(peer_addr.unwrap(), peer, kill_switch_rx)) {
                    Ok(_) => {
                        let rx = DropReceiver::new(rx, Some((kill_switch_tx, peer_addr.unwrap())));
                        Ok(tonic::Response::new(Box::pin(rx)))
                    }
                    Err(err) => {
                        let msg = format!("error, {}", err);
                        Err(tonic::Status::new(tonic::Code::Unavailable, msg))
                    }
                }
            }
            None => Err(tonic::Status::new(
                tonic::Code::InvalidArgument,
                "name is invalid",
            )),
        }
    }

    /// broadcast_event:
    async fn broadcast_event(
        &self,
        request: tonic::Request<proto::Event>,
    ) -> Result<tonic::Response<proto::MessageAck>, tonic::Status> {
        tracing::info!("broadcast event received");
        let peer_addr = request.remote_addr();

        let event: proto::Event = request.into_inner();
        match self.state.lock().await.dispatch.send(ToServer::Event(peer_addr.unwrap(), event)) {
            Ok(_) => {
                Ok(tonic::Response::new(proto::MessageAck {
                    status: "SENT".to_string(),
                }))
            }
            Err(err) => {
                let msg = format!("error, {}", err);
                Err(tonic::Status::new(tonic::Code::Unavailable, msg))
            }
        }
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

pub async fn start_server(opts: ServerOpts) -> Result<(), Box<dyn std::error::Error>> {
    let (signal_tx, signal_rx) = signal_channel();

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

    let addr: SocketAddr = opts.server_listen_addr.parse().unwrap();

    let state = Arc::new(Mutex::new(Shared::new()));
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
