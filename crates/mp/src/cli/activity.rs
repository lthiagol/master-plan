use clap::Args;

/// M180 S2: `mp activity` — bounded read of the project activity
/// journal. The default limit matches the on-disk retention cap
/// (500). Raul passes `--limit 5` for the Overview-tab preview.
#[derive(Args, Debug)]
pub struct ActivityCmd {
    /// Maximum number of events to return (newest first). Defaults
    /// to the on-disk retention cap (500). Raul passes 5 for the
    /// Overview preview.
    #[arg(long)]
    pub limit: Option<usize>,
}
