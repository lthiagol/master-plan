use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum IdeaCmd {
    Create {
        #[arg(long)]
        title: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        tags: Option<String>,
        #[arg(long)]
        source: Option<String>,
    },
    List {
        #[arg(long)]
        status: Option<String>,
    },
    Show {
        id: String,
    },
    Update {
        id: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        tags: Option<String>,
    },
    Dismiss {
        id: String,
    },
    Archive {
        id: String,
    },
    Remove {
        id: String,
    },
    Promote {
        id: String,
        #[arg(long)]
        to_milestone: bool,
        #[arg(long)]
        to_backlog: bool,
        #[arg(long)]
        to_track: Option<String>,
    },
}
