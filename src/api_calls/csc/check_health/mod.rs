use crate::macros::command;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use anyhow::Result;

command!(serde_json::json!({
    "title":"Error",
    "body":"Unfortunately the agent wasn't able to get node health status right now."
}));

#[derive(Serialize, Deserialize)]
pub struct Output {}

impl Output {
    pub async fn create(_data: Value) -> Result<Value> {
        // We need to find an action that the worker can perform to show that it is functional
        // Probably analyzing the output of pm2 status and kubectl get nodes 
        println!("Health check performed!");
        Ok(
            serde_json::to_value(true)?
        )
    }
}