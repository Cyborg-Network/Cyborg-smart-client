
mod specs;
mod cli;

use procedural_macros::Command;

#[derive(Debug, Clone, Command)]
pub enum CliUtils {
    Specs,
    Cli,
}

#[derive(Deserialize)]
struct Input {
    command: Vec<String>,
}
