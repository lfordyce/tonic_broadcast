use uuid;

use futures::{SinkExt, StreamExt};
use tokio::io;
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};

use async_stream::stream;
use futures_util::pin_mut;
use tonic::transport::Channel;
use tonic::Request;

use proto::broadcast_client::BroadcastClient;
use proto::{Message, User};

use prost_types::Timestamp;

pub mod proto {
    tonic::include_proto!("proto");
}

use crate::ClientOpts;

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
            tracing::error!("Failed to get username from. Client disconnected.");
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
        // msg.timestamp
        // let output = format!("{}: {}", msg, msg);
        match msg.event {
            Some(event) => {
                match event {
                    proto::event::Event::Log(event_log) => {
                        let user = match event_log.user {
                            Some(user) => user.name,
                            _ => "anonymous".to_string(),
                        };
                        let msg = match event_log.message {
                            Some(msg) => msg.content,
                            _ => "FAILED".to_string(),
                        };
                        println!("{}: {}", user, msg);
                    },
                    proto::event::Event::Join(event_join) => {
                        let user = match event_join.user {
                            Some(user) => user.name,
                            _ => "anonymous".to_string(),
                        };
                        println!("{}: has joined the chat", user);
                    },
                    _ => {
                        println!("UNKNOWN");
                    }
                }
            },
            _ => {
                println!("oops??");
            }
        }
        // println!("MESSAGE = {:?}", msg);
    }
    Ok(())
}
