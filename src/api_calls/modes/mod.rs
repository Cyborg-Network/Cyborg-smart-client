mod specs;
mod cli;
mod remove;

use proc_macros::Command;
use serde::Deserialize;


#[derive(Debug, Clone, Command)]
pub enum ApiTypes {
    Cli,
    Specs,
    Remove,
}


#[derive(Deserialize)]
struct Input {
    command: Vec<String>,
}