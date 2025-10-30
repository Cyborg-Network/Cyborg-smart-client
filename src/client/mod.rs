use std::{process::Stdio, sync::{Arc, RwLock}};
use tokio::process::Command;
use crate::{
    TASK_CONTAINER_PREFIX, 
    api::{HealthStatus, Init, Usage, dbus::watch_for_zk_stage_update, logs}, 
    auth::{self, WsAuthRequest}, crypto::{decode_polkadot_address, read_task_owner}, 
    error_handling::{ClientError, construct_client_error_message}, 
    formats::{self, OptionalStatusCode, OptionalUuid}
};
use anyhow::{/*anyhow, bail,  Context,  */Result};
use futures::stream::SplitSink;
use futures_util::{SinkExt, StreamExt/* , TryFutureExt */};
/* use http::{Request, Uri}; */
use serde::{Deserialize, Serialize};
use serde_json::{/* from_str,  ser, */from_str, Value};
use tokio::{
    task::JoinHandle,
    sync::Mutex,
    io::{AsyncReadExt, AsyncWriteExt}, 
    select, 
    net::{TcpListener, TcpStream}
};
use tokio_tungstenite::{
    WebSocketStream, accept_async, tungstenite::Message
};
use uuid::Uuid;
use http::{Response, StatusCode};
// use local_ip_address::local_ip;

const HTTP_ADDR: &str = "0.0.0.0:8080";
const WS_ADDR: &str = "0.0.0.0:8081";

const CREATE_CONTAINER_KEYS_SCRIPT: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/create_container_key.sh"));
const DEPOSIT_CONTAINER_KEYS_SCRIPT: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/deposit_container_key.sh"));

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
struct SshKeypair {
    pub_key: String,
    priv_key: String
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

#[derive(Deserialize, Serialize, Debug)]
#[serde(tag = "endpoint")]
enum WsMessageFormat {
    Request(WsApiRequest),
    Auth(WsAuthRequest),
    Test(WsTestRequest),
}

#[derive(Deserialize, Serialize, Debug)]
struct WsApiRequest {
    target_ip: String,
    request_type: WsApiRequestType,
}

#[derive(Deserialize, Serialize, Debug)]
enum WsApiRequestType {
    Init,
    Usage,
    CreateContainerKey(CreateContainerKeyRequest),
    DepositContainerKey(DepositContainerKeyRequest),
}

#[derive(Deserialize, Serialize, Debug)]
struct DepositContainerKeyRequest {
    task_id: String,
    key: String,
}

#[derive(Deserialize, Serialize, Debug)]
struct CreateContainerKeyRequest {
    task_id: String,
}

#[derive(Deserialize, Serialize, Debug)]
struct WsAuthResponse{
    response_type: String,
    node_public_key: String,
}

#[derive(Deserialize, Serialize, Debug)]
struct WsTestRequest {
    target_ip: String,
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

        let task_owner = read_task_owner()
            .map_err(|e| println!("Failed to read task owner: {}", e)).unwrap();

        let public_key_bytes = decode_polkadot_address(task_owner.task_owner.as_str()).unwrap(); 

        let (ws_sender, mut ws_receiver) = ws_stream.split();

        let zk_stage: Arc<Mutex<u8>> = Arc::new(Mutex::new(0));

        let zk_stage_updating_clone = Arc::clone(&zk_stage);

        tokio::spawn(async move {
            println!("Print 1");
            if let Err(e) = watch_for_zk_stage_update(zk_stage_updating_clone).await {
                eprintln!("Error watching for zk stage update: {}", e);
            }
        });

        //let mut diffie_hellman_key: Option<[u8; 32]> = None;

        // Mutexes and RwLocks
        let ws_sender = Arc::new(Mutex::new(ws_sender));
        let log_storage: logs::LogsStorage = Arc::new(Mutex::new(Vec::new()));
        let diffie_hellman_key = Arc::new(RwLock::new(None));

        let mut streaming_task: Option<JoinHandle<()>> = None;

        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(Message::Text(message)) => {
                    println!("Message received: {}", message);
                    match from_str::<WsMessageFormat>(&message) {
                        Ok(msg) => match msg {
                            WsMessageFormat::Request(request) => {
                                match request.request_type {

                                    WsApiRequestType::Usage => {
                                        if streaming_task.is_none() {
                                            let stream_usage_diffie_hellman_key = Arc::clone(&diffie_hellman_key);
                                            let stream_usage_log_storage = Arc::clone(&log_storage);
                                            let stream_usage_ws_sender = Arc::clone(&ws_sender);
                                            let stream_usage_zk_stage = Arc::clone(&zk_stage);

                                            streaming_task = Some(tokio::spawn(async move {
                                               let sender = stream_usage_ws_sender; 

                                               if let Err(e) = Usage::stream_usage( 
                                                    &sender, 
                                                    stream_usage_diffie_hellman_key, 
                                                    stream_usage_log_storage,
                                                    stream_usage_zk_stage,
                                                ).await {
                                                    let mut sender_guard = sender.lock().await;
                                                    let _ =sender_guard.send(
                                                       Message::Text(construct_client_error_message(e))).await;
                                                }
                                            }));
                                        }
                                    }

                                    WsApiRequestType::Init => {
                                        let init_res = Init::return_init_message(&diffie_hellman_key).await;
                                        
                                        match init_res {
                                            Ok(init_message) => {
                                                let mut sender_guard = ws_sender.lock().await;
                                                if let Err(e) = sender_guard.send(Message::Text(init_message)).await {
                                                    println!("Failed to send init message: {}", e);
                                                }
                                            }
                                            Err(e) => {
                                                let mut sender_guard = ws_sender.lock().await;
                                                if let Err(e) = sender_guard.send(Message::Text(construct_client_error_message(e))).await {
                                                    println!("Failed to send init message: {}", e);
                                                }
                                            }
                                        }
                                    }

                                    WsApiRequestType::CreateContainerKey(request) => {
                                        if let Err(e) = handle_create_container_ssh_key(request.task_id, &ws_sender).await {
                                            println!("Failed to send request key response message, sending client error.");
                                            let mut sender_guard = ws_sender.lock().await;
                                            sender_guard.send(Message::Text(construct_client_error_message(e))).await.unwrap();
                                        }
                                    }

                                    WsApiRequestType::DepositContainerKey(request) => {
                                        if let Err(e) = handle_deposit_container_ssh_key(request.task_id, request.key, &ws_sender).await {
                                            println!("Failed to send request key response message, sending client error.");
                                            let mut sender_guard = ws_sender.lock().await;
                                            sender_guard.send(Message::Text(construct_client_error_message(e))).await.unwrap();
                                        }
                                    }
                                }
                            }
                            WsMessageFormat::Auth(request) => {
                              let auth_res = auth::construct_auth_response(request, &diffie_hellman_key, &log_storage, public_key_bytes);

                              match auth_res {
                                Ok(response) => {
                                    let mut sender_guard = ws_sender.lock().await;
                                    if let Err(e) = sender_guard.send(Message::Text(response)).await {
                                        println!("Failed to send auth message: {}", e);
                                    }
                                }
                                Err(e) => {
                                    let mut sender_guard = ws_sender.lock().await;
                                    if let Err(e) = sender_guard.send(Message::Text(construct_client_error_message(e))).await {
                                        println!("Failed to send auth message: {}", e);
                                }
                                }
                              }
                            }
                            WsMessageFormat::Test(_) => {
                               let mut sender_guard = ws_sender.lock().await;
                                if let Err(e) = sender_guard.send(Message::Text("Test".to_string())).await {
                                    println!("Failed to send auth message: {}", e);
                                } 
                            }
                        }
                        _ => { 
                            let mut sender_guard = ws_sender.lock().await;
                            if let Err(e) = sender_guard.send(Message::Text(construct_client_error_message(ClientError::InvalidRequestError))).await {
                                println!("Failed to send auth message: {}", e);
                            } 
                        }
                    }
                }
                _ => {}
            }
        }
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

fn construct_container_name(task_id: String) -> String {
    format!("{}-{}", *TASK_CONTAINER_PREFIX, task_id)
}

async fn handle_create_container_ssh_key(task_id: String, sender: &Arc<Mutex<SplitSink<WebSocketStream<TcpStream>, Message>>>) -> Result<(), ClientError> {
    let container_name = construct_container_name(task_id);
    
    let mut child = Command::new("bash")
        .arg("-s")
        .arg("--")
        .arg(container_name)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| ClientError::CreateContainerKeyError(e.to_string()))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(CREATE_CONTAINER_KEYS_SCRIPT.as_bytes()).await
            .map_err(|e| ClientError::CreateContainerKeyError(e.to_string()))?;
    }

    let output = child.wait_with_output().await
        .map_err(|e| ClientError::CreateContainerKeyError(e.to_string()))?
        .stdout;

    let stout_str = String::from_utf8(output)
        .map_err(|e| ClientError::CreateContainerKeyError(e.to_string()))?;

    let data = serde_json::from_str::<SshKeypair>(&stout_str)
        .map_err(|e| ClientError::CreateContainerKeyError(e.to_string()))?;

    let mut sender_guard = sender.lock().await;
    if let Err(e) = sender_guard.send(Message::Text(
        serde_json::to_string(&data)
            .map_err(|e| ClientError::CreateContainerKeyError(e.to_string()))?
    )).await {
        println!("Failed to send init message: {}", e);
    }

    Ok(())
}

async fn handle_deposit_container_ssh_key(task_id: String, key: String, sender: &Arc<Mutex<SplitSink<WebSocketStream<TcpStream>, Message>>>) -> Result<(), ClientError> {
    let container_name = construct_container_name(task_id);

    let mut child = Command::new("bash")
        .arg("-s")
        .arg("--")
        .arg(container_name)
        .arg(key)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| ClientError::DepositContainerKeyError(e.to_string()))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(DEPOSIT_CONTAINER_KEYS_SCRIPT.as_bytes()).await
            .map_err(|e| ClientError::DepositContainerKeyError(e.to_string()))?;
    }

    let output = child.wait_with_output().await
        .map_err(|e| ClientError::CreateContainerKeyError(e.to_string()))?
        .stdout;

    let stout_str = String::from_utf8(output)
        .map_err(|e| ClientError::DepositContainerKeyError(e.to_string()))?;

    let data = serde_json::from_str::<SshKeypair>(&stout_str)
        .map_err(|e| ClientError::DepositContainerKeyError(e.to_string()))?;

    let mut sender_guard = sender.lock().await;
    if let Err(e) = sender_guard.send(Message::Text(
        serde_json::to_string(&data)
            .map_err(|e| ClientError::DepositContainerKeyError(e.to_string()))?
    )).await {
        println!("Failed to send init message: {}", e);
    }

    Ok(())
}