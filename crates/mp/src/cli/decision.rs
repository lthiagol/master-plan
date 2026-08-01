use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum DecisionCmd {
    Add {
        #[arg(long)]
        summary: String,
        #[arg(long)]
        context: Option<String>,
        #[arg(long)]
        milestone: Option<String>,
    },
    List,
    Remove {
        id: String,
    },
}
