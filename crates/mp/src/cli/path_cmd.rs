use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum PathSubCmd {
    Pin {
        milestone: String,
        #[arg(long)]
        before: Option<String>,
        #[arg(long)]
        rank: Option<u32>,
        #[arg(long)]
        reason: Option<String>,
    },
    Unpin {
        milestone: String,
    },
    ListPins {
        #[arg(long)]
        milestone: Option<String>,
    },
    Focus {
        milestone: String,
        #[arg(long)]
        through: Option<String>,
    },
    ClearFocus,
    Suggest,
}
