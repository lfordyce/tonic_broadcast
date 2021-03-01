use std::error::Error;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use uuid;

use futures::{future::BoxFuture, SinkExt, StreamExt};
use tokio::io;
use tokio::sync::{mpsc, Mutex};
use tokio::time::Instant;
use tokio_util::codec::{BytesCodec, Framed, FramedRead, FramedWrite, LinesCodec, LinesCodecError};

use async_stream::{stream, try_stream};
use futures_core::stream::Stream;
use futures_util::pin_mut;
use tonic::transport::{Channel, Endpoint};
use tonic::Request;

use proto::broadcast_client::BroadcastClient;
use proto::event::Event::{Join, Leave, Log, ServerShutdown};
use proto::{Message, User};

use tonic::body::Body;

use prost_types::Timestamp;

pub mod proto {
    tonic::include_proto!("proto");
}

use crate::ClientOpts;

pub async fn client_run(opts: ClientOpts) -> Result<(), Box<dyn std::error::Error>> {
    // let addr: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9090);
    // let mut client = BroadcastClient::connect("http://127.0.0.1:9090").await?;

    // let channel = Endpoint::from_static(&*opts.server_addr).connect().await?;
    // let main_client = BroadcastClient::new(channel);
    let main_client = BroadcastClient::connect(opts.server_addr).await?;

    let mut client1 = main_client.clone();
    let mut client2 = main_client.clone();

    let token = uuid::Uuid::new_v4();

    let mut prompt = FramedWrite::new(io::stdout(), LinesCodec::new());
    // Send a prompt to the client to enter their username.
    prompt.send("Please enter your username:").await?;

    let mut input = FramedRead::new(io::stdin(), LinesCodec::new());

    // Read the first line from the `LineCodec` stream to get the username.
    let username = match input.next().await {
        Some(Ok(line)) => line,
        // We didn't get a line so we return early here.
        _ => {
            // tracing::error!("Failed to get username from {}. Client disconnected.", addr);
            return Ok(());
        }
    };

    let username1 = username.clone();
    let username2 = username.clone();

    tokio::spawn(async move {
        let outbound = stream! {
            let mut lines = FramedRead::new(io::stdin(), LinesCodec::new());
            while let Some(line) = lines.next().await {
                let msg = match line {
                    Ok(msg) => msg,
                    Err(e) => {
                        println!("error {:?}", e);
                        return;
                    }
                };
                yield msg;
            }
        };
        pin_mut!(outbound);

        while let Some(value) = outbound.next().await {
            let event_log = proto::event::EventLog {
                user: Some(User {
                    token: token.to_string(),
                    name: username1.to_string(),
                }),
                message: Some(Message {
                    id: token.to_string(),
                    content: value,
                }),
            };

            let timestamp = Timestamp::from(std::time::SystemTime::now());

            let event = proto::Event {
                timestamp: Some(timestamp),
                event: Some(proto::event::Event::Log(event_log)),
            };
            if let Err(e) = client1.broadcast_event(Request::new(event)).await {
                println!("failed to broadcast message: {:?}", e);
                return;
            }
        }
    });

    let join_request = proto::JoinRequest {
        user: Some(User {
            token: token.to_string(),
            name: username2.to_string(),
        }),
    };

    let mut stream = client2
        .join_stream(Request::new(join_request))
        .await?
        .into_inner();
    while let Some(msg) = stream.message().await? {
        // let output = format!("{}: {}", msg, msg);
        println!("MESSAGE = {:?}", msg);
    }
    Ok(())
}

// async fn agent_loop(client: Arc<Mutex<&mut BroadcastClient<Channel>>>) {
//     let conn = Connect {
//         user: Some(User { token: "12w34rr56tyy7u".to_string(), name: "bruce".to_string() }),
//         active: true,
//     };
//     let mut client = client.lock().await;
//     if let Err(e) = client.create_stream(Request::new(conn)).await {
//         println!("an error occurred; error = {:?}", e)
//     }
// }

// pub async fn agent_loop(channel: Channel, mut ctrl: mpsc::Receiver<tonic::Request<Event>>) -> BoxFuture<'static, Result<(), Box<dyn Error>>> {
// pub async fn agent_loop(client: &mut BroadcastClient<Channel>, mut ctrl: mpsc::Receiver<tonic::Request<Event>>) {
// pub async fn agent_loop(client: BroadcastClient<Channel>, mut ctrl: mpsc::Receiver<tonic::Request<Event>>) {
//     // let client = BroadcastClient::new(channel);
//     tokio::pin!(client);
//
//     loop {
//         tokio::select! {
//             event = ctrl.recv() => {
//                 match event {
//                     Some(r) => {
//                         if let Err(e) = client.broadcast_event(r).await {
//                             println!("an error occurred; error = {:?}", e);
//                             break;
//                         }
//                     },
//                     None => {
//                         println!("something happened...????");
//                     }
//                 }
//             }
//             // res = {
//             //     let conn = Connect {
//             //         user: Some(User { token: "12w34rr56tyy7u".to_string(), name: "bruce".to_string() }),
//             //         active: true,
//             //     };
//             //     client.create_stream(Request::new(conn)).await
//             // } => {
//             //     match res {
//             //         Ok(data) => {
//             //             let mut stream = data.into_inner();
//             //             loop {
//             //                 match stream.message().await {
//             //                     Ok(message) => {
//             //                         println!("NOTE = {:?}", message);
//             //                     }
//             //                     _ => { break; }
//             //                 }
//             //             }
//             //         },
//             //         Err(e) => {
//             //             println!("found error: {:?}", e);
//             //             break;
//             //         }
//             //     }
//             // }
//             // _ = connection(&mut client) => {
//             //     println!("connection() completed first")
//             //     // match res {
//             //     //     Ok(()) => {
//             //     //         println!("all good");
//             //     //     },
//             //     //     Err(e) => {
//             //     //         println!("encountered error: {:?}", e);
//             //     //         break;
//             //     //     }
//             //     // }
//             // }
//         }
//     }
// }

// async fn connection(client: &mut BroadcastClient<Channel>) -> Result<tonic::Response<tonic::codec::Streaming<Event>>, tonic::Status> {
// async fn connection(client: &mut BroadcastClient<Channel>) {
//     let conn = Connect {
//         user: Some(User { token: "12w34rr56tyy7u".to_string(), name: "bruce".to_string() }),
//         active: true,
//     };
//
//     match client.create_stream(Request::new(conn)).await {
//         Ok(data) => {
//             let mut stream = data.into_inner();
//             loop {
//                 match stream.message().await {
//                     Ok(message) => {
//                         println!("NOTE = {:?}", message);
//                     }
//                     _ => { break; }
//                 }
//             }
//         }
//         Err(e) => {
//             println!("found error: {:?}", e);
//         }
//     }
// }
