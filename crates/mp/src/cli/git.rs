use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum GitCmd {
    Status,
    SuggestMessage,
    Commit {
        #[arg(long)]
        message: Option<String>,
    },
}
