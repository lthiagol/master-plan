use clap::Args;

/// M180 S2: `mp overview` — consolidated project-health snapshot.
#[derive(Args, Debug)]
pub struct OverviewCmd {
    /// Summary mode: drops the bounded `path` / `inbox` /
    /// `activity` previews (status-strip / Overview-tab header).
    #[arg(long)]
    pub summary: bool,
}
