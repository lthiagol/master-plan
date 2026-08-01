use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum SessionCmd {
    Start {
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        title: Option<String>,
    },
    Show {
        id: Option<String>,
    },
    List,
    Focus {
        id: String,
    },
    Unfocus,
    Archive {
        id: String,
        #[arg(long)]
        force: bool,
    },
    Export {
        id: String,
    },
    Promote {
        id: String,
        #[arg(long)]
        milestone: Option<String>,
    },
}
