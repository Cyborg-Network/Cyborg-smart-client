use std::time::Duration;

use crate::{
    api::{HealthStatus, Init, Usage}, 
    crypto::{compute_diffie_hellman_secret, decode_polkadot_address, encrypt_message, generate_server_ephemeral_keypair, verify_signature}, 
    formats::{self, OptionalStatusCode, OptionalUuid}
};
use futures_util::stream::SplitSink;
use anyhow::{/*anyhow, bail,  Context,  */anyhow, ensure, Result};
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
use x25519_dalek::PublicKey;
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

enum WsMessageFormat {
    Request(WsRequestMessageFormat),
    Auth(WsAuthMessageFormat),
}

enum WsRequestMessageType {
    Usage,
    Init,
    Unknown(String),
}

impl WsRequestMessageType {
    fn from_str(message: &str) -> Self {
        match message {
            "USAGE" => WsRequestMessageType::Usage,
            "INIT" => WsRequestMessageType::Init,
            _ => WsRequestMessageType::Unknown(message.to_string()),
        }
    }
}

struct WsRequestMessageFormat {
    request_type: String,
}

struct WsAuthMessageFormat {
    pub timestamp: String,
    pub timestamp_signature: Vec<u8>,
    pub public_key: PublicKey,
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

        let public_key_bytes = decode_polkadot_address("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY").unwrap(); 

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        let mut diffie_hellman_key: Option<[u8; 32]> = None;

        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(msg) => {
                    match msg {
                        Message::Text(data) => {
                            let decoded_message = decode_ws_message(data);
                            match decoded_message {
                                Ok(WsMessageFormat::Request(request)) => {
                                    let msg_enum = WsRequestMessageType::from_str(&request.request_type);
                                    match msg_enum {
                                        WsRequestMessageType::Usage => { let _ = stream_usage(&mut ws_sender, &diffie_hellman_key).await; }
                                        WsRequestMessageType::Init => { let _ = get_init(&mut ws_sender, &diffie_hellman_key).await; }
                                        WsRequestMessageType::Unknown(s) => {
                                            let _ = ws_sender
                                                .send(Message::Text(format!("Received unexpected message: {}", s)))
                                                .await;
                                        }
                                    }
                                }
                                Ok(WsMessageFormat::Auth(auth)) => {
                                    let _ = verify_signature( auth.timestamp, &auth.timestamp_signature, &public_key_bytes)
                                        .expect("Signature varification failed.");

                                    let server_keypair = generate_server_ephemeral_keypair();

                                    let server_public_key_bytes = server_keypair.1.to_bytes();

                                    diffie_hellman_key = Some(compute_diffie_hellman_secret(server_keypair.0, auth.public_key.into()));

                                    let serialized_message = &hex::encode(&server_public_key_bytes);

                                    let message = "AUTH|".to_string() + &serialized_message;

                                    let _ = ws_sender
                                        .send(Message::Text(message))
                                        .await;
                                }
                                Err(e) => println!("Error decoding message format: {}", e),
                            }
                            
                        }
                        Message::Close(_) => {
                            println!("Received close, closing");
                            break;
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

fn decode_ws_message(msg: String) -> Result<WsMessageFormat> {
    //part 0: request type
    //part 1: message (in this case a timestamp - Date.now() in js)
    //part 2: signature of the timestamp
    //part 3: public key of the current instance of the frontend (x25519 key, NOT a polkadot address - pdot address be fetched from substrate)
    let parts: Vec<&str> = msg.split('|').collect();

    let request_type = parts[0].to_string();

    match parts.len() {
        1 => {
            ensure!(request_type != "AUTH", anyhow!(format!("Request type and request format mismatch! Message: {}", msg)));
            Ok(WsMessageFormat::Request(WsRequestMessageFormat { request_type: parts[0].to_string()}))
        }
        4 => {
            // verify message type is correct
            ensure!(request_type == "AUTH", anyhow!(format!("Request type and request format mismatch! Message: {}", msg)));

            // get rid of message prefix if it is there
            let timestamp_signature;
            if parts[2].starts_with("0x") {
                timestamp_signature = hex::decode(parts[2].trim_start_matches("0x")).expect("Failed to decode signature");
            } else {
                timestamp_signature = hex::decode(parts[2]).expect("Failed to decode signature");
            }

            // convert x25519 public key into the correct format
            let mut array = [0u8; 32];
            println!("{}", parts[3]);
            let public_key_vec = hex::decode(parts[3]).expect("Failed to decode public key");

            if public_key_vec.len() != 32 {
                println!("Public key length must be 32 bytes.");
            };

            array.copy_from_slice(&public_key_vec);

            let public_key = PublicKey::from(array);

            // timestamp itself will remain a string, since signature verification and timestamp conversion need it to be different
            // types, so it makes more sense to let the functions do the conversion themselves

            // return
            Ok(WsMessageFormat::Auth(WsAuthMessageFormat { timestamp: parts[1].to_string(), timestamp_signature, public_key }))
        }
        _ => Err(anyhow!(format!("Invalid message format: {}", msg))),
    }
}

async fn stream_usage(
    stream: &mut SplitSink<WebSocketStream<TcpStream>, Message>,
    diffie_hellman_key: &Option<[u8; 32]>,
) -> Result<()> {
    if let Some(diffie_hellman_key) = diffie_hellman_key {
        let mut query_interval = time::interval(Duration::from_secs(2));

        loop {
            query_interval.tick().await;

            let metric_item = Usage::get_usage_snapshot().await?;

            let serialized_message = serde_json::to_string(&metric_item)?;

            let encrypted_message = encrypt_message("USAGE", diffie_hellman_key, serialized_message);

            stream
                .send(Message::Text(encrypted_message))
                .await?
        }
    } else {
        Err(anyhow!("No diffie hellman key found, skipping usage stream"))
    }
}

async fn get_init(
   stream: &mut SplitSink<WebSocketStream<TcpStream>, Message>,
   diffie_hellman_key: &Option<[u8; 32]>
) -> Result<()> {
    if let Some(diffie_hellman_key) = diffie_hellman_key {
        let spec_item = Init::get_init().await?;

        let serialized_message = serde_json::to_string(&spec_item)?;

        let encrypted_message = encrypt_message("INIT", diffie_hellman_key, serialized_message);

        let init = stream
            .send(Message::Text(encrypted_message))
            .await.map_err(|_| anyhow::anyhow!("Init failed"));

        init
    } else {
        Err(anyhow!("No diffie hellman key found, skipping init"))
    }
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