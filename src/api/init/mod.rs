use serde::Serialize;
use async_recursion::async_recursion;

use super::{health_status, location, specs};

#[derive(Serialize)]
pub struct Init{
    location: location::Location,
    status: bool,
    specs: specs::Specs,
}

impl Init {
    #[async_recursion]
    pub async fn get_init() -> Result<Init, anyhow::Error> {
        let location = location::Location::get_location().await?;
        let status = health_status::HealthStatus::get_health_status().await?;
        let specs = specs::Specs::get_specs().await?;

        Ok(
            Init{
                location,
                status,
                specs,
            }
        )
    }
}
