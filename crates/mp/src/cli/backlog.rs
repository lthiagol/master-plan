use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum BacklogCmd {
    Add {
        #[arg(long)]
        desc: String,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        suggested_when: Option<String>,
        #[arg(long)]
        priority: Option<String>,
    },
    /// M112 WP1: read-only list of backlog items with AND-combined
    /// filters. Output is `{ items: [...] }` regardless of result count
    /// — empty backlog returns `{ items: [] }`, not `null`.
    List {
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        priority: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
    },
    Show {
        id: String,
    },
    Resolve {
        id: String,
        #[arg(long)]
        into_milestone: Option<String>,
        #[arg(long)]
        wont_fix: bool,
        #[arg(long)]
        reason: Option<String>,
    },
    Promote {
        id: String,
        #[arg(long)]
        to_milestone: bool,
        #[arg(long)]
        to_track: Option<String>,
    },
}
