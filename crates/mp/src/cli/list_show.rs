use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum ListTarget {
    Milestones {
        /// Groom-state filter (all, pending, in-progress, partial, done, blocked, grooming)
        #[arg(long)]
        filter: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        spec_status: Option<String>,
        #[arg(long)]
        include_archived: bool,
        /// Named filter preset (force-bypassed, etc.)
        #[arg(long)]
        preset: Option<String>,
        /// Field comparison filter, e.g. spec_status==ready or execution_status!=done
        #[arg(long, value_delimiter = ',')]
        r#where: Vec<String>,
        /// Include full detail inline (steps, acceptance_criteria, evidence)
        #[arg(long, value_delimiter = ',')]
        include: Vec<String>,
        /// M112 S3: slice the first N items (ascending id order).
        #[arg(long)]
        take: Option<usize>,
        /// M112 S3: project a dotted path to a leaf value across the array.
        /// Returns a flat array (one entry per item, in current sort order).
        #[arg(long)]
        select: Option<String>,
        /// M112 S3: order by a top-level field (e.g. `milestone.id`,
        /// `milestone.priority`). Prefix the field with `-` for descending
        /// (`--sort -id` = newest milestone first). Composes with `--take`.
        #[arg(long, allow_hyphen_values = true)]
        sort: Option<String>,
    },
    Tracks {
        /// Include individual items (id, title, status) in output
        #[arg(long)]
        items: bool,
    },
    Steps {
        #[arg(long)]
        milestone: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        include_archived: bool,
        /// M112 S3: slice the first N items.
        #[arg(long)]
        take: Option<usize>,
        /// M112 S3: project a dotted path to a leaf value across the steps array.
        #[arg(long)]
        select: Option<String>,
        /// M112 S3: order by a top-level step field (e.g. `id`, `status`).
        /// Prefix with `-` for descending. Composes with `--take`.
        #[arg(long, allow_hyphen_values = true)]
        sort: Option<String>,
    },
    Archived {
        #[arg(long, name = "type")]
        entity_type: Option<String>,
    },
    Backlog {
        #[arg(long)]
        status: Option<String>,
    },
    Decisions,
}

#[derive(Subcommand, Debug)]
pub enum ShowTarget {
    Milestone {
        id: String,
        /// Rollup counts for steps/ACs/findings + review_state (replaces jq health checks)
        #[arg(long)]
        summary: bool,
    },
    Archived {
        entity_type: String,
        id: String,
        #[arg(long)]
        kind: Option<String>,
    },
}
