use std::net::SocketAddr;

use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tonic::transport::Server;

use proto::broadcast_server::{Broadcast, BroadcastServer};
use proto::{Connect, User};

pub mod proto {
    tonic::include_proto!("proto");
}

#[derive(Debug)]
pub struct Service {
    peers: Arc<Mutex<Vec<Connection>>>,
}

#[derive(Debug)]
struct Peer {
    connections: Vec<Connection>,
}

impl Peer {
    fn new() -> Self {
        Peer {
            connections: Vec::new(),
        }
    }
}

impl Service {
    pub fn new(peers: Arc<Mutex<Vec<Connection>>>) -> Self {
        Service { peers }
    }
}

#[derive(Debug, Clone)]
pub struct Connection {
    stream: mpsc::Sender<Result<proto::Message, tonic::Status>>,
    id: String,
    active: bool,
}

#[tonic::async_trait]
impl Broadcast for Service {
    type CreateStreamStream = mpsc::Receiver<Result<proto::Message, tonic::Status>>;

    /// create_stream:
    async fn create_stream(
        &self,
        request: tonic::Request<Connect>,
    ) -> Result<tonic::Response<Self::CreateStreamStream>, tonic::Status> {
        let req = request.into_inner();

        match req.user {
            Some(user) => {
                let (mut tx, rx) = mpsc::channel::<Result<proto::Message, tonic::Status>>(4);

                let conn = Connection {
                    stream: tx,
                    active: true,
                    id: user.id,
                };
                let mut state = self.peers.lock().await;
                state.push(conn);

                Ok(tonic::Response::new(rx))
            }
            None => Err(tonic::Status::new(
                tonic::Code::InvalidArgument,
                "name is invalid",
            )),
        }
    }

    /// broadcast_message:
    async fn broadcast_message(
        &self,
        request: tonic::Request<proto::Message>,
    ) -> Result<tonic::Response<proto::Close>, tonic::Status> {
        let mut req: proto::Message = request.into_inner();

        let mut state = self.peers.lock().await;

        for mut conn in state.to_vec() {
            let req = req.clone();
            tokio::task::spawn_blocking(move || {
                if conn.active {
                    tokio::spawn(async move {
                        if let Err(e) = conn.stream.send(Ok(req)).await {
                            println!("an error occurred; error = {:?}", e)
                        }
                        println!("sending message...");
                    });
                }
            });
        }
        Ok(tonic::Response::new(proto::Close {}))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr: SocketAddr = "[::1]:20000".parse().unwrap();
    println!("BroadcastService listening on: {}", addr);

    let peers = Arc::new(Mutex::new(Vec::new()));
    let svc = BroadcastServer::new(Service::new(peers));

    Server::builder().add_service(svc).serve(addr).await?;

    Ok(())
}
