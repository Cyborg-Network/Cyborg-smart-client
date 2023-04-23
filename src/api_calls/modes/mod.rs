mod specs;
mod cli;

use proc_macros::Command;
use serde::Deserialize;


#[derive(Debug, Clone, Command)]
pub enum Command {
    Cli,
    Specs,
}


#[derive(Deserialize)]
struct Input {
    command: Vec<String>,
}