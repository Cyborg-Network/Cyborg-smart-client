mod remove;
mod check_health;
mod usage;
mod location;
mod init;
mod specs;

use proc_macros::Command;

#[derive(Debug, Clone, Command)]
pub enum Command {
    /* Remove, */
    CheckHealth,
    Usage,
    Location,
    Init,
}
