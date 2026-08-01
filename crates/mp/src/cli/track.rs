use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum TrackCmd {
    List {
        /// Include individual items (id, title, status) in output
        #[arg(long)]
        items: bool,
    },
    Show {
        kind: String,
    },
    Add {
        kind: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        problem: Option<String>,
        #[arg(long)]
        verification: Option<String>,
        #[arg(long)]
        done_when: Option<String>,
        #[arg(long)]
        step: Vec<String>,
    },
    Start {
        kind: String,
        id: String,
    },
    Done {
        kind: String,
        id: String,
        #[arg(long)]
        evidence: Option<String>,
    },
    Cancel {
        kind: String,
        id: String,
    },
    Promote {
        kind: String,
        id: String,
        #[arg(long)]
        to_milestone: bool,
    },
    #[command(subcommand)]
    Archive(ArchiveCmd),
    #[command(subcommand)]
    Restore(RestoreCmd),
    #[command(subcommand)]
    Purge(PurgeCmd),
}

#[derive(Subcommand, Debug)]
pub enum ArchiveCmd {
    Milestone { id: String },
    TrackItem { kind: String, id: String },
}

#[derive(Subcommand, Debug)]
pub enum RestoreCmd {
    Archived {
        entity_type: String,
        id: String,
        #[arg(long)]
        kind: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum PurgeCmd {
    Archived {
        entity_type: Option<String>,
        id: Option<String>,
        #[arg(long)]
        older_than: Option<String>,
        #[arg(long)]
        confirm: bool,
    },
}
