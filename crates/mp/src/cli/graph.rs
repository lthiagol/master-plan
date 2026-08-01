use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum GraphCmd {
    Explain { milestone: String },
}
