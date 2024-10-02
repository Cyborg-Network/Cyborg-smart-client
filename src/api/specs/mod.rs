use anyhow::{Context, Result};
use pkg_version::{pkg_version_major, pkg_version_minor, pkg_version_patch};
use serde::Serialize;
use sysinfo::{CpuExt, NetworksExt, System, SystemExt};

mod memory;

use crate::macros::make_request;

#[derive(Serialize)]
pub struct Specs {
    cpus: Vec<String>,
    memory: String,
    networks: Vec<String>,
    os: String,
    linux_version: String,
    kernel: String,
    serverhostname: String,
    csc_connected: bool,
    csc_version: String,
    ip: String,
    /* timezone: String, */
}

impl Specs {
    pub async fn get_specs() -> Result<Specs> {
        let mut sys = System::new_all();
        sys.refresh_all();

        let mut cpus = sys
            .cpus()
            .iter()
            .map(|cpu| cpu.brand().to_string())
            .collect::<Vec<_>>();
        cpus.sort_unstable();
        cpus.dedup();

        let memory = memory::get_memory().await?;

        let networks = sys
            .networks()
            .iter()
            .map(|(name, _networkdata)| name.to_string())
            .collect::<Vec<_>>();

        let os = sys.long_os_version().context("Failed to get OS version")?;
        let linux_version = sys.os_version().context("Failed to get short OS version")?;
        let kernel = sys
            .kernel_version()
            .context("Failed to get kernel version")?;
        let serverhostname = sys.host_name().context("Failed to get server hostname")?;
        // potential injection. Don't run eval on returned JSON
        let ip = make_request!("public ip address".to_string(), "https://ifconfig.me")
            .text()
            .await
            .context("Failed to get public ip address as string")?;

        // Commented out as it requires sudo privileges and can't automatically run
        /* let timezone = output!("timezone", "sudo", &["cat", "/etc/timezone"]); */

        Ok(
            Specs {
                cpus,
                memory,
                networks,
                os,
                linux_version,
                kernel,
                serverhostname,
                csc_connected: true,
                csc_version: format!(
                    "{}.{}.{}",
                    pkg_version_major!(),
                    pkg_version_minor!(),
                    pkg_version_patch!()
                ),
                ip,
                /* timezone, */
            }
        )
    }
}
