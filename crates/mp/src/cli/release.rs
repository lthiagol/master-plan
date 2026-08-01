use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum ReleaseCmd {
    List,
    Map,
    Show {
        version: String,
    },
    Ship {
        version: String,
        #[arg(long)]
        force: bool,
    },
}
