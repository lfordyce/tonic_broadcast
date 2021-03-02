use std::error::Error;
use std::net::SocketAddr;
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
use std::str::FromStr;

pub async fn client_run(opts: ClientOpts) -> Result<(), Box<dyn std::error::Error>> {
    let channel = Channel::from_shared(opts.server_addr)?;
    let main_client = BroadcastClient::connect(channel).await?;

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

    tracing::info!("client username entered {}", username);

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
