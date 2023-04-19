use log::{error, info};
use rust_socketio::{ClientBuilder, Payload, RawClient};
use serde_json::json;
use std::time::Duration;

fn main() {
    // Initialize the logger
    env_logger::init();

    // Define a callback which is called when a payload is received
    // This callback gets the payload as well as an instance of the
    // socket to communicate with the server
    let callback = |payload: Payload, socket: RawClient| {
        match payload {
            Payload::String(str) => info!("Received: {}", str),
            Payload::Binary(bin_data) => info!("Received bytes: {:#?}", bin_data),
        }
        socket
            .emit("test", json!({"got ack": true}))
            .unwrap_or_else(|err| error!("Failed to emit: {}", err));
    };

    // Get a socket that is connected to the admin namespace
    let socket = ClientBuilder::new("http://localhost:4200")
        .namespace("/admin")
        .on("test", callback)
        .on("error", |err, _| error!("Error: {:#?}", err))
        .connect()
        .expect("Connection failed");

    // Emit to the "foo" event
    let json_payload = json!({"token": 123});
    socket
        .emit("foo", json_payload)
        .unwrap_or_else(|err| error!("Failed to emit: {}", err));

    // Define a callback, that's executed when the ack got acked
    let ack_callback = |message: Payload, _| {
        info!("Yehaa! My ack got acked?");
        info!("Ack data: {:#?}", message);
    };

    let json_payload = json!({"myAckData": 123});
    // Emit with an ack
    socket
        .emit_with_ack("test", json_payload, Duration::from_secs(2), ack_callback)
        .unwrap_or_else(|err| error!("Failed to emit with ack: {}", err));

    socket
        .disconnect()
        .unwrap_or_else(|err| error!("Failed to disconnect: {}", err));
}
