use anyhow::Result;
use serde::Serialize;
use sysinfo::{CpuExt, CpuRefreshKind, RefreshKind, System, SystemExt, DiskExt};
use std::time::Duration;
use tokio::time::sleep;

#[derive(Serialize, Debug)]
pub struct Usage {
    title: &'static str,
    cpu_usage: f32,
    mem_usage: u64,
    disk_usage: u64,
}

impl Usage {
    pub async fn get_usage_snapshot() -> Result<Usage> {
        let mut system = System::new_with_specifics(
            RefreshKind::new()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory()
                .with_disks(),
        );

        // Refreshes and sleeps are needed for this to work properly as it measures usage over time

        system.refresh_cpu();
        system.refresh_disks_list();

        sleep(Duration::from_secs(1)).await;

        system.refresh_cpu();
        
        let metric_item = Usage {
            title: "Usage",
            cpu_usage: system.cpus().iter().map(|cpu| cpu.cpu_usage()).sum::<f32>()
                / system.cpus().len() as f32,
            mem_usage: system.used_memory(),
            disk_usage: system.disks().iter()
                .map(|disk| {
                    let total_space = disk.total_space();
                    let available_space = disk.available_space();
                    total_space - available_space // Calculate used space
                })
                .sum(),
        };
        
        println!("{:#?}", metric_item);

        Ok(metric_item)
    }
}
