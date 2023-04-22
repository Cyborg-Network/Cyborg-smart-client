use anyhow::{Context, Result};
use byte_unit::{Byte, ByteUnit};
use pkg_version::{pkg_version_major, pkg_version_minor, pkg_version_patch};
use serde::Serialize;
use serde_json::Value;
use sysinfo::{CpuExt, NetworksExt, System, SystemExt};

use crate::macros::{command, make_request};

mod security;

command!(serde_json::json!({
    "title":"Error",
    "body":"Unfortunately the csc wasn't able to get server information right now"
}));

#[derive(Serialize)]
struct Output {
    cpus: Vec<String>,
    memory: String,
    networks: Vec<String>,
    os: String,
    serverhostname: String,
    csc_connected: bool,
    csc_version: String,
    ip: String,
    security: u8,
}

impl Output {
    pub async fn create(_data: Value) -> Result<Value> {
        let mut sys = System::new_all();
        sys.refresh_all();

        let mut cpus = sys
            .cpus()
            .iter()
            .map(|cpu| cpu.brand().to_string())
            .collect::<Vec<_>>();
        cpus.sort_unstable();
        cpus.dedup();

        let memory = Byte::from_unit(sys.total_memory() as f64, ByteUnit::KB)
            .context("Failed to get total memory as kilobytes")?
            .get_appropriate_unit(false)
            .to_string();

        let networks = sys
            .networks()
            .iter()
            .map(|(name, _networkdata)| name.to_string())
            .collect::<Vec<_>>();

        let os = sys.long_os_version().context("Failed to get OS version")?;
        let serverhostname = sys.host_name().context("Failed to get server hostname")?;
        // potential injection. Don't run eval on returned JSON
        let ip = make_request!("public ip address".to_string(), "https://ifconfig.me")
            .text()
            .await
            .context("Failed to get public ip address as string")?;

        Ok(serde_json::to_value(Output {
            cpus,
            memory,
            networks,
            os,
            serverhostname,
            csc_connected: true,
            csc_version: format!(
                "{}.{}.{}",
                pkg_version_major!(),
                pkg_version_minor!(),
                pkg_version_patch!()
            ),
            ip,
            security: security::get_security_status().await?,
        })
        .unwrap())
    }
}

