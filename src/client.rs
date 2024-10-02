use std::time::Duration;

use crate::{
    api::{HealthStatus, Init, Usage},
    /* config::Configuration, */
    formats::{self, OptionalStatusCode, OptionalUuid},
};
use futures_util::stream::SplitSink;
use anyhow::{/*anyhow, bail,  Context,  */Result};
use futures_util::{SinkExt, StreamExt/* , TryFutureExt */};
/* use http::{Request, Uri}; */
use serde::{Deserialize, Serialize};
use serde_json::{/* from_str,  ser, */Value};
use tokio::{select, net::{TcpListener, TcpStream}};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, time::{self}};
use tokio_tungstenite::{
    accept_async,
    WebSocketStream,
    tungstenite::Message
};
use uuid::Uuid;
use http::{Response, StatusCode};
// use local_ip_address::local_ip;

const HTTP_ADDR: &str = "127.0.0.1:8080";
const WS_ADDR: &str = "127.0.0.1:8081";

#[derive(Deserialize, Serialize)]
/// the required format for messages within text websocket frames
struct Messages {
    #[serde(rename = "type")]
    request_type: RequestType,
    /// randomly generated id for the message
    #[serde(with = "formats::SerdeUuid")]
    id: Uuid,
    /// reference to the previous message, if any
    #[serde(rename = "ref")]
    reference_id: OptionalUuid,
    /// ms since UNIX epoch
    timestamp: String,
    /// a status code
    status_code: OptionalStatusCode,
    args: Vec<String>,
    /// timeout value in ms
    timeout: Option<u64>,
    data: Value,
}

#[derive(Deserialize, Serialize)]
pub enum RequestType {
    /// a new request
    #[serde(rename = "syn")]
    Syn,
    #[serde(rename = "ack")]
    /// a response (acknowledgement) to a previous request
    Ack,
}

enum WsTextMessage {
    Usage,
    Init,
    Unknown(String),
}

impl WsTextMessage {
    fn from_str(message: &str) -> Self {
        match message {
            "USAGE" => WsTextMessage::Usage,
            "INIT" => WsTextMessage::Init,
            _ => WsTextMessage::Unknown(message.to_string()),
        }
    }
}

async fn handle_http_request(mut stream: TcpStream) {
    let mut buf = [0; 1024];
    let bytes_read = stream.read(&mut buf).await.unwrap();

    let error_response = "HTTP/1.1 404 NOT FOUND\r\nContent-Length: 0\r\n\r\n";

    // Try to parse the request from the raw buffer
    let request_str = String::from_utf8_lossy(&buf[..bytes_read]);

    if let Some(request_line) = request_str.lines().next() {
        let parts: Vec<&str> = request_line.split_whitespace().collect();

        if parts.len() == 3 {
            let method = parts[0];
            let path = parts[1];

            // Only handle GET requests for simplicity
            if method == "GET" && path == "/check-health" {
                let is_healthy = HealthStatus::get_health_status().await.ok();

                if let Some(is_healthy) = is_healthy {

                    let response = if is_healthy {
                        Response::builder()
                            .status(StatusCode::OK)
                            .header("Content-Type", "application/json")
                            .body("{\"isActive\": \"true\"}".to_string())
                            .unwrap()
                    } else {
                        Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .header("Content-Type", "application/json")
                            .body("{\"isActive\": \"false\"}".to_string())
                            .unwrap()
                    };

                    let response_str = format!(
                        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nContent-Type: {}\r\n\r\n{}",
                        response.status().as_u16(),
                        response.status().canonical_reason().unwrap_or(""),
                        response.body().len(),
                        response.headers().get("Content-Type").unwrap().to_str().unwrap(),
                        response.body(),
                    );

                    stream.write_all(response_str.as_bytes()).await.unwrap();
                    stream.flush().await.unwrap();
                    return;
                } else{
                    return;
                }
            } else {
                stream.write_all(error_response.as_bytes()).await.unwrap();
                stream.flush().await.unwrap();
            }
        }
    }

    stream.write_all(error_response.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();
}

async fn handle_ws_connections(stream: TcpStream) {
    if let Ok(ws_stream) = accept_async(stream).await {
        println!("WebSocket handshake has been successfully completed");

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(msg) => {
                    match msg {
                        Message::Text(data) => {
                            let msg_enum = WsTextMessage::from_str(&data);
                            match msg_enum {
                                WsTextMessage::Usage => { let _ = stream_usage(&mut ws_sender).await; }
                                WsTextMessage::Init => { let _ = get_init(&mut ws_sender).await; }
                                WsTextMessage::Unknown(s) => {
                                    let _ = ws_sender
                                        .send(Message::Text(format!("Received unexpected message: {}", s)))
                                        .await;
                                }
                            }
                        }
                        Message::Close(_) => {
                            println!("Received close, closing");
                            break;
                        }
                        Message::Ping(_) | Message::Pong(_) => {
                            println!("Received unexpected frame: {:?}", msg);
                        }
                        Message::Binary(_) => {
                            println!("Received unexpected frame: {:?}", msg);
                        }
                        _ => {
                            println!("Received unexpected frame: {:?}", msg);
                        }
                    }
                }
                Err(e) => {
                    println!("Error in receiving message: {}", e);
                }
            }
        }
    }
}

async fn stream_usage(
    stream: &mut SplitSink<WebSocketStream<TcpStream>, Message>,
) -> Result<()> {

    let mut query_interval = time::interval(Duration::from_secs(2));

    loop {
        query_interval.tick().await;

        let metric_item = Usage::get_usage_snapshot().await?;

        let serialized_message = serde_json::to_string(&metric_item)?;

        stream
            .send(Message::Text(serialized_message))
            .await?
    }
}

async fn get_init(
   stream: &mut SplitSink<WebSocketStream<TcpStream>, Message>,
) -> Result<()> {
    let spec_item = Init::get_init().await?;

    let serialized_message = serde_json::to_string(&spec_item)?;

    let init = stream
        .send(Message::Text(serialized_message))
        .await.map_err(|_| anyhow::anyhow!("Init failed"));

    init
}

/// connects to websocket, handles message frames, and starts scheduled actions
pub async fn run_client(/* config: &Configuration */) -> Result<()> {
    /* let config: Configuration = config.clone();
    let request = create_request(
        &config.base.websocket_server_url,
        &config.base.user_token,
        &config.base.csc_uuid,
    )?; */

    let http_listener = TcpListener::bind(HTTP_ADDR).await.unwrap();
    let ws_listener = TcpListener::bind(WS_ADDR).await.unwrap();

    let http_task = tokio::spawn({
        async move {
            loop {
                let (stream, _) = http_listener.accept().await.unwrap();
                tokio::spawn(handle_http_request(stream));
            }
        }
    });

    let ws_task = tokio::spawn(async move {
        loop {
            let (stream, _) = ws_listener.accept().await.unwrap();
            tokio::spawn(handle_ws_connections(stream));
        }
    });

    //let (mut input_tx, input_rx) = futures_channel::mpsc::unbounded();
    //let (output_tx, output_rx) = futures_channel::mpsc::unbounded();
  
    // forward output from a transmitter to the websocket
    //let forward_output = output_rx.map(Ok).forward(&mut ws_sender);
    // read from the websocket and forward its input to the transmitter
    //let forward_input = ws_receiver.map(Ok).forward(&mut input_tx);

    //select! {
        //_ = message_handling => (),
        //_ = forward_input => (),
        //_ = forward_output => (),
    //}

    select! {
        _ = http_task => {},
        _ = ws_task => {},
    }

    Ok(())
}

/* 
/// creates websocket request. taken approximately from [tungstenite docs](https://docs.rs/tungstenite/0.17.1/src/tungstenite/client.rs.html#216-237)
fn create_request(url: &str, user_token: &str, csc_uuid: &str) -> Result<Request<()>> {
    let uri = url.parse::<Uri>().unwrap();
    let authority = uri
        .authority()
        .ok_or(anyhow!("Failed to get authority from uri"))?
        .to_string();
    let host = authority
        .find('@')
        .map(|idx| authority.split_at(idx + 1).1)
        .unwrap_or_else(|| &authority);

    if host.is_empty() {
        bail!("Failed to get host from uri");
    }
    let r: [u8; 16] = rand::random();
    let key = base64::encode(&r);

    Ok(Request::builder()
        .method("GET")
        .header("Host", host)
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", key)
        .header("userToken", user_token)
        .header("nodeID", csc_uuid)
        .uri(uri)
        .body(())?)
} */
/* 
/// processes websocket message with text frame
pub async fn process_message(data: String, default_timeout: u64) -> Result<String> {

    if data == "ping" {
        return Ok("pong".to_string());
    
       }
    let message: Messages = deserialize_message(data).context("Failed to deserialize message")?;

    // chain building command, then spawn a task with specified timeout
    let result = futures::future::ready(
        build_command(message.args)
            .context("Failed to get command from message arguments")
            .map_err(|e| {
                serde_json::json!({
                    "title": "Argument Error",
                    "body": "Unable to deserialize arguments",
                    "error_details": format!("{:#}", e),
                })
            }),
    )
    .and_then(|cmd| async move {
        tokio::time::timeout(
            Duration::from_millis(message.timeout.unwrap_or(default_timeout)),
            tokio::spawn(cmd.run(message.data)).map_err(|e| {
                serde_json::json!({
                    "title":"Internal Error",
                    "body":"Error occured in processing",
                    "error_details": format!("{:#}", e)
                })
            }),
        )
        .await
        .map_err(|_| {
            serde_json::json!({
                "title":"Internal Error",
                "body":"The command did not complete within the timeout period"
            })
        })
    })
    .await;
    let result = flatten(flatten(result));

    // response
    // temporary fix, removing the timeout from return value. issues with parsing it in server (was using eval for json parse)
    let a = 
    // serialize_message(
        
        Messages {
        request_type: RequestType::Ack,
        id: Uuid::new_v4(),
        reference_id: OptionalUuid(Some(message.id)),
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
            .to_string(),
        status_code: if result.is_err() {
            OptionalStatusCode(Some(http::StatusCode::INTERNAL_SERVER_ERROR))
        } else {
            OptionalStatusCode(Some(http::StatusCode::OK))
        },
        args: vec![],
        timeout: None,
        // simply return the ok value or the err value. the status is already encoded within the message and the status code
        data: result.unwrap_or_else(std::convert::identity),
      };
    let mut a = serde_json::to_value(a).unwrap();
    let a = a.as_object_mut().unwrap();
    a.remove("timeout");
    Ok(Value::Object(a.clone()).to_string())
    // )
    // .context("Failed to serialize output")
}


fn deserialize_message(msg: String) -> Result<Messages> {
    Ok(from_str(&msg).context("Failed to get message from string")?)
}

/// helper method. builds a command enum from a vector of arguments
pub fn build_command(args: Vec<String>) -> Result<api_calls::Command> {
    api_calls::Command::from_args(&args)
}

/// flattens a Result (due to .flatten being unstable)
fn flatten<K, E>(r: Result<Result<K, E>, E>) -> Result<K, E> {
    match r {
        Ok(Ok(k)) => Ok(k),
        Ok(Err(e)) => Err(e),
        Err(e) => Err(e),
    }
} */