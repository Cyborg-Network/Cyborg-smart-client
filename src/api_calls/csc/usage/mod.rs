use crate::macros::command;
use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use sysinfo::{CpuExt, CpuRefreshKind, RefreshKind, System, SystemExt, DiskExt};
use std::time::Duration;
use tokio::time::sleep;

command!(serde_json::json!({
    "title":"Error",
    "body":"Unfortunately the agent wasn't able to query the nodes usage metrics right now."
}));

#[derive(Serialize, Debug)]
struct Output {
    title: &'static str,
    cpu_usage: f32,
    mem_usage: u64,
    disk_usage: u64,
}

impl Output {
    async fn create( _data: Value ) -> Result<Value> {
        let mut system = System::new_with_specifics(
            RefreshKind::new()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory()
                .with_disks(),
        );

        system.refresh_cpu();
        system.refresh_disks_list();

        sleep(Duration::from_secs(1)).await;

        system.refresh_cpu();
        
        let metric_item = Output {
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

        Ok(
            serde_json::to_value(metric_item)?
        )
    }
}
