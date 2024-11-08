use futures_util::stream::StreamExt;
use zbus::{proxy, Connection};
use std::sync::Arc;
use tokio::sync::Mutex;

#[proxy(
    default_service = "com.cyborg.CyborgAgent",
    default_path = "/com/cyborg/CyborgAgent",
    interface = "com.cyborg.AgentZkInterface"
)]
trait ZkUpdateManager {
    // Defines signature for D-Bus signal named `ZkUpdate`
    #[zbus(signal)]
    fn zk_update(&self, stage: u8) -> zbus::Result<()>;
}

pub async fn watch_for_zk_stage_update(zk_stage: Arc<Mutex<u8>>) -> zbus::Result<()> {
    let connection = Connection::system().await?;
    // `ZkUpdateManagerProxy` is generated from `ZkUpdateManager` trait

    let systemd_proxy = ZkUpdateManagerProxy::new(&connection).await?;
    // Method `receive_job_new` is generated from `job_new` signal
    let mut zk_update_stream = systemd_proxy.receive_zk_update().await?;

    while let Some(msg) = zk_update_stream.next().await {
        // struct `ZkUpdateArgs` is generated from `zk_update` signal function arguments
        let args: ZkUpdateArgs = msg.args().expect("Error parsing message");

        println!("Zk update received: path={}", args.stage);

        *zk_stage.lock().await = args.stage;
    }

    panic!("Stream ended unexpectedly");
}