use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum PlanCmd {
    Show,
    Set {
        #[arg(long)]
        planning_status: Option<String>,
        #[arg(long)]
        planning_phase: Option<String>,
        #[arg(long)]
        target_version: Option<String>,
        #[arg(long)]
        stack: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        name: Option<String>,
    },
    Goals {
        #[command(subcommand)]
        cmd: GoalsCmd,
    },
    Nongoals {
        #[command(subcommand)]
        cmd: NongoalsCmd,
    },
    Principles {
        #[command(subcommand)]
        cmd: PrinciplesCmd,
    },
    Gaps {
        id: String,
    },
    Coverage {
        id: String,
    },
    InferDeps {
        id: String,
    },
    Relocate {
        old: std::path::PathBuf,
        new: std::path::PathBuf,
    },
    Diff {
        #[arg(long)]
        since_handoff: bool,
        #[arg(long)]
        since: Option<String>,
        #[arg(long)]
        git: Option<String>,
        #[arg(long)]
        markdown: bool,
    },
    #[command(subcommand)]
    Metrics(MetricsCmd),
    /// Soft WARN-only lint for broad-scope AC/step verification strings.
    VerifyLint,
    /// M121 S9: AC verification integrity pre-flight. Walks every AC's
    /// verification field and resolves cargo test targets, Makefile
    /// targets, bash/python scripts. Surfaces unresolvable symbols.
    VerifyAc {
        id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum PrinciplesCmd {
    Add { text: String },
    Remove { text: String },
    Set { json: String },
}

#[derive(Subcommand, Debug)]
pub enum GoalsCmd {
    Add { text: String },
    Remove { text: String },
    Set { json: String },
}

#[derive(Subcommand, Debug)]
pub enum NongoalsCmd {
    Add { text: String },
    Remove { text: String },
    Set { json: String },
}

#[derive(Subcommand, Debug)]
pub enum MetricsCmd {
    Show,
    Set {
        #[arg(long)]
        lines_of_code: Option<u64>,
        #[arg(long)]
        unit_tests: Option<u64>,
        #[arg(long)]
        integration_tests: Option<u64>,
        #[arg(long)]
        coverage_percent: Option<f64>,
    },
}
