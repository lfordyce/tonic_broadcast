use std::net::{SocketAddr, Ipv4Addr, IpAddr};
use std::error::Error;
use std::sync::Arc;

use uuid;

use futures::{StreamExt, SinkExt, future::BoxFuture};
use tokio::io;
use tokio::sync::{mpsc, Mutex};
use tokio::time::Instant;
use tokio_util::codec::{Framed, FramedRead, FramedWrite, LinesCodec, LinesCodecError, BytesCodec};

use tonic::transport::{Endpoint, Channel};
use tonic::Request;
use async_stream::{stream, try_stream};
use futures_core::stream::Stream;
use futures_util::pin_mut;

use proto::broadcast_client::BroadcastClient;
use proto::{User, Message};
use crate::proto::event::Event::{Join, Log, Leave, ServerShutdown};
use tonic::body::Body;

use prost_types::Timestamp;

pub mod proto {
    tonic::include_proto!("proto");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // let addr: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9090);
    // let mut client = BroadcastClient::connect("http://127.0.0.1:9090").await?;

    let channel = Endpoint::from_static("http://127.0.0.1:9090").connect().await?;
    let main_client = BroadcastClient::new(channel);

    let mut client1 = main_client.clone();
    let mut client2 = main_client.clone();

    let token = uuid::Uuid::new_v4();

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
                user: Some(User { token: token.to_string(), name: "bruce".to_string() }),
                message: Some(Message { id: token.to_string(), content: value }),
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

    // let conn = Connect {
    //     user: Some(User { token: "12w34rr56tyy7u".to_string(), name: "bruce".to_string() }),
    //     active: true,
    // };

    let join_request = proto::JoinRequest {
        user: Some(User { token: token.to_string(), name: "bruce".to_string() }),
    };

    // let timestamp = Timestamp::from(std::time::SystemTime::now());
    //
    // let event = proto::Event {
    //     timestamp: Some(timestamp),
    //     event: Some(proto::event::Event::Join(event_join)),
    // };

    let mut stream = client2.join_stream(Request::new(join_request)).await?.into_inner();
    while let Some(msg) = stream.message().await? {
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

