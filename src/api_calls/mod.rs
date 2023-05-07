
mod modes;

use proc_macros::Command;


#[derive(Debug, Clone, Command)]
pub enum ApiTypes {
    Modes(modes::Command),
}

