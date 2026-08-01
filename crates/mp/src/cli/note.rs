use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum NoteCmd {
    Add {
        #[arg(long)]
        title: String,
        /// Inline body text. For a file/stdin body, prefer --body-file (the
        /// @file/@- sentinels on --body are kept for backward-compat but
        /// collide with bodies that legitimately start with '@').
        #[arg(long)]
        body: Option<String>,
        /// Read the body from a file path (or '-' for stdin). Preferred over
        /// --body @file: there's no ambiguity with inline text. Expands a
        /// leading '~/' to $HOME.
        #[arg(long = "body-file", value_name = "PATH")]
        body_file: Option<String>,
        #[arg(long)]
        to: Option<String>,
    },
}
