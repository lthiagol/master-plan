use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum SpecsCmd {
    List,
    Show {
        domain: String,
    },
    Init {
        domain: String,
        #[arg(long)]
        title: Option<String>,
    },
    #[command(subcommand)]
    Delta(DeltaCmd),
}

#[derive(Subcommand, Debug)]
pub enum SpecCmd {
    /// Condensed review-oriented projection of a milestone spec (outcome,
    /// problem, scope, ACs with coverage + evidence + force-bypass, open
    /// questions, coverage gaps). Reuses M79 --fields for slicing.
    Review { milestone: String },
    /// What spec fields changed since the milestone's last approval (review).
    /// Anchors on git history of the milestone file.
    Diff { milestone: String },
}

#[derive(Subcommand, Debug)]
pub enum BrownfieldCmd {
    Scan {
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        query: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum DeltaCmd {
    Rebase { milestone: String },
}
