use serde::Serialize;
use anyhow::Result;
use serde_json::Value;
use crate::macros::command;
use async_recursion::async_recursion;

use crate::api_calls;

use super::{check_health, location, specs};

command!(serde_json::json!({
    "title":"Error",
    "body":"Unfortunately the agent wasn't able to initialize the node right now."
}));

#[derive(Serialize)]
struct Output{
    location: location::Output,
    status: bool,
    specs: specs::Output,
}

impl Output {
    #[async_recursion]
    async fn create(_data: Value) -> Result<Value> {
        let location = api_calls::csc::Command::Location.run(Value::Null).await.ok();
        let status = api_calls::csc::Command::CheckHealth.run(Value::Null).await.ok();
        let specs = api_calls::modes::Command::Specs.run(Value::Null).await.ok();

        match (location, status, specs) {
            (Some(location), Some(status), Some(specs)) => Ok(
                serde_json::to_value(Output {
                    location: serde_json::from_value::<location::Output>(location)?,
                    status: serde_json::from_value::<bool>(status)?,
                    specs: serde_json::from_value::<specs::Output>(specs)?,
                })?
            ),
            _ => Err(anyhow::anyhow!("Failed to initialize"))
        }
    }
}
