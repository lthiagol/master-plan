use anyhow::Result;

use crate::cli::Cli;

mod agent;
mod dispatch;
mod interview;
mod spec;
mod watch_control;

/// Discover the plan context and dispatch one CLI command.
pub fn run(cli: Cli) -> Result<()> {
    dispatch::run(cli)
}
