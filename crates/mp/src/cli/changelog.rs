use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum ChangelogCmd {
    Show {
        #[arg(long)]
        version: Option<String>,
    },
    Add {
        entry: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        section: String,
        #[arg(long)]
        milestone: Option<String>,
    },
    Init,
    Generate {
        #[arg(long)]
        version: String,
    },
}
