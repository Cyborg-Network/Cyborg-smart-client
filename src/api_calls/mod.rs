pub mod csc;
mod modes;

use proc_macros::Command;

#[derive(Debug, Clone, Command)]
pub enum Command {
    Modes(modes::Command),
    Csc(csc::Command),
}
