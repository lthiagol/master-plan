use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum BriefCmd {
    Todo,
    List,
    Show {
        id: Option<String>,
    },
    Edit {
        id: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        status: Option<String>,
    },
    Add {
        #[arg(long)]
        title: String,
        #[arg(long)]
        prompt: Option<String>,
        #[arg(long)]
        required: bool,
    },
    Rm {
        id: String,
    },
    Skip {
        id: String,
    },
    Done,
    Reopen,
    Promote {
        id: String,
        #[arg(long)]
        to_idea: bool,
        #[arg(long)]
        to_backlog: bool,
    },
    Import {
        #[arg(long)]
        from_file: String,
    },
}
