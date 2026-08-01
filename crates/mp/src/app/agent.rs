use anyhow::Result;

use crate::cli::{AgentCmd, HarnessCmd, OutputFormat};
use crate::commands::agent;
use crate::paths::PlanContext;

pub(super) fn run(ctx: &PlanContext, cmd: AgentCmd, format: OutputFormat) -> Result<()> {
    match cmd {
        AgentCmd::Role { role, clear } => agent::cmd_agent_role(ctx, role, clear, format),
        AgentCmd::Harness { cmd } => match cmd {
            HarnessCmd::List => agent::cmd_agent_harness_list(ctx, format),
            HarnessCmd::StartCommand {
                name,
                model,
                thinking_level,
            } => agent::cmd_agent_harness_start_command(ctx, &name, model, thinking_level, format),
        },
    }
}
