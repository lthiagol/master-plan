use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum InterviewCmd {
    Checklist {
        #[arg(long = "checklist-type", visible_alias = "type", value_name = "TYPE")]
        checklist_type: String,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        draft: bool,
    },
    Gaps {
        id: Option<String>,
        #[arg(long)]
        kind: Option<String>,
    },
}
