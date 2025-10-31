use std::path::PathBuf;

use ::clap::Parser;
use anyhow::Result;
use once_cell::sync::Lazy;

mod clap;
use crate::clap::App;
mod cli;
mod api;
mod client;
mod config;
mod formats;
mod macros;
mod crypto;
mod auth;
mod error_handling;
//#[cfg(test)]
//mod unit_tests;

lazy_static::lazy_static! {
    /// uses [home::home_dir] to determine $HOME or its equivalent
    pub static ref CONFIG_PATH: PathBuf = home::home_dir()
        .expect("Failed to get home directory")
        .join(".config/cyborg/config.toml");

    // pub static ref RELEASE_SERVER_URL: String = "https://localhost:9000/releases".to_string()
    //     + &format!(
    //         "v{}.{}.{}/",
    //         pkg_version_major!(),
    //         pkg_version_minor!(),
    //         pkg_version_patch!()
    //     )
    //     + "scripts/";
}

pub struct Paths {
    pub task_owner: PathBuf,
    pub miner_config: PathBuf,
    pub logs: PathBuf,
}

pub static TASK_CONTAINER_PREFIX: Lazy<String> = Lazy::new(|| {
    std::env::var("TASK_CONTAINER_PREFIX").expect("TASK_CONTAINER_PREFIX not set")
});

pub static PATHS: Lazy<Paths> = Lazy::new(|| {
    Paths{
        task_owner: std::env::var("TASK_OWNER_FILE_PATH").expect("TASK_OWNER_FILE_PATH not set").into(),
        miner_config: std::env::var("IDENTITY_FILE_PATH").expect("IDENTITY_FILE_PATH not set").into(),
        logs: std::env::var("LOG_FILE_PATH").expect("LOG_FILE_PATH not set").into(),
    }
});

#[tokio::main]
async fn main() -> Result<()> {
    // parse command line arguments
    let app = App::parse();

    Lazy::force(&TASK_CONTAINER_PREFIX);
    Lazy::force(&PATHS);

    // initialize logger
    let config_str = include_str!("log.yml");

    let config = serde_yaml::from_str(config_str).unwrap();

    log4rs::init_raw_config(config).unwrap();

    println!("Running");

    match app {
        App::CreateConfig { force, user_token } => {
            config::create_config(&CONFIG_PATH, force, user_token)?;
        }
        App::Run => {
            /* let config = config::load_config(&CONFIG_PATH)?; */
            client::run_client(/* &config */).await.unwrap();
        }
    }
    Ok(())
}
