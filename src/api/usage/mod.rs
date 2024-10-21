use anyhow::Result;
use serde::Serialize;
use sysinfo::{CpuExt, CpuRefreshKind, RefreshKind, System, SystemExt, DiskExt};
use std::time::Duration;
use tokio::time::sleep;
use crate::api::logs;

#[derive(Serialize, Debug)]
pub struct Usage {
    title: &'static str,
    cpu_usage: f32,
    mem_usage: u64,
    disk_usage: u64,
    recent_logs: Vec<String>,
}

#[derive(Serialize, Debug)]
pub struct MemoryAndDiskInfo {
    pub total_memory: u64,
    pub total_disk: u64,
}

use std::process::Command;
use std::str;

// this is here for the same reason mentioned in storage.rs
pub fn return_disk_usage() -> u64 {
    let output = Command::new("df")
        .arg("--block-size=1") 
        .arg("-B1")  
        .output()
        .expect("Failed to execute df command");

    let stdout = str::from_utf8(&output.stdout).expect("Invalid UTF-8");

    let mut total_used_space: u64 = 0;

    for line in stdout.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();

        if let Some(filesystem) = parts.get(0) {
            if filesystem.starts_with("/dev/") {
                if let Some(used_space) = parts.get(2) {  // Get the used space (3rd column)
                    total_used_space += used_space.parse::<u64>().unwrap_or(0);
                }
            }
        }
    }

    total_used_space
}

impl Usage {
    pub async fn get_usage_snapshot(log_storage: logs::LogsStorage) -> Result<Usage> {
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

        let _ = system.disks().iter()
                .map(|disk| {
                    let total_space = disk.total_space();
                    let available_space = disk.available_space();
                    total_space - available_space // Calculate used space
                });
        
        let metric_item = Usage {
            title: "Usage",
            cpu_usage: system.cpus().iter().map(|cpu| cpu.cpu_usage()).sum::<f32>()
                / system.cpus().len() as f32,
            mem_usage: system.used_memory() * 1024,
            disk_usage: return_disk_usage(),
            recent_logs: logs::retrieve_new_logs(log_storage)
        };
        
        println!("{:#?}", metric_item);

        Ok(metric_item)
    }
}
