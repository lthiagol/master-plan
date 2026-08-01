use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum AnnotationCmd {
    Create {
        target: String,
        kind: String,
        body: String,
        author: String,
    },
    List {
        #[arg(long)]
        open: bool,
        #[arg(long)]
        target: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        author: Option<String>,
    },
    Show {
        id: String,
    },
    Update {
        id: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        author: Option<String>,
    },
    Resolve {
        id: String,
    },
    Reopen {
        id: String,
    },
    Remove {
        id: String,
    },
    Addressed {
        id: String,
    },
}
