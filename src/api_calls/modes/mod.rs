mod cli;
mod specs;

use proc_macros::Command;

#[derive(Debug, Clone, Command)]
pub enum Command {
    Cli(cli::Command),
    Specs,
}
