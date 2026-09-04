//! Clap CLI surface for `mp`, split by command group for locality.
//! Public items are re-exported at `crate::cli::*` so call sites stay stable.

use clap::{Parser, Subcommand, ValueEnum};

mod parsers;
pub mod path;
mod path_cmd;
#[allow(unused_imports)] // stable pub(crate) path: crate::cli::files_value_parser
pub(crate) use parsers::files_value_parser;

mod activity;
mod agent;
mod annotation;
mod autopilot;
mod backlog;
mod brief;
mod changelog;
mod config_cmd;
mod decision;
mod edit;
mod execution;
mod git;
mod graph;
mod idea;
mod interview;
mod list_show;
mod milestone;
mod note;
mod overview;
mod plan;
mod release;
mod reviews;
mod scratch;
mod session;
mod specs;
mod track;

pub use activity::ActivityCmd;
pub use agent::{AgentCmd, HarnessCmd, SkillCmd};
pub use annotation::AnnotationCmd;
pub use autopilot::{
    AutopilotCmd, AutopilotConfigCmd, AutopilotNoteCmd, AutopilotSessionCmd, AutopilotStartArgs,
    NoteArgs, TransitionArgs,
};
pub use backlog::BacklogCmd;
pub use brief::BriefCmd;
pub use changelog::ChangelogCmd;
pub use config_cmd::ConfigCmd;
pub use decision::DecisionCmd;
pub use edit::EditCmd;
pub use execution::ExecutionCmd;
pub use git::GitCmd;
pub use graph::GraphCmd;
pub use idea::IdeaCmd;
pub use interview::InterviewCmd;
pub use list_show::{ListTarget, ShowTarget};
pub use milestone::{
    BulkCmd, BulkDependsOnAction, ChallengeCmd, CriterionCmd, DesignDecisionCmd, MilestoneCmd,
    QuestionCmd, StageCmd, StepCmd, WpCmd,
};
pub use note::NoteCmd;
pub use overview::OverviewCmd;
pub use path_cmd::PathSubCmd;
pub use plan::{GoalsCmd, MetricsCmd, NongoalsCmd, PlanCmd, PrinciplesCmd};
pub use release::ReleaseCmd;
pub use reviews::{CommentCmd, FindingCmd, ReviewsCmd};
pub use scratch::ScratchCmd;
pub use session::SessionCmd;
pub use specs::{BrownfieldCmd, DeltaCmd, SpecCmd, SpecsCmd};
pub use track::{ArchiveCmd, PurgeCmd, RestoreCmd, TrackCmd};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum OutputFormat {
    #[default]
    Json,
    /// Debug: raw on-disk JSON passthrough (`show milestone`) or GraphViz DOT (`graph`).
    Raw,
}

#[derive(Parser, Debug)]
#[command(name = "mp", about = "Master Plan CLI", version)]
pub struct Cli {
    #[arg(long, global = true)]
    pub project_root: Option<std::path::PathBuf>,

    #[arg(long, global = true)]
    pub plan_dir: Option<std::path::PathBuf>,

    #[arg(long, global = true, value_enum)]
    pub format: Option<OutputFormat>,

    #[arg(long, global = true)]
    pub quiet: bool,

    #[arg(long, global = true)]
    pub verbose: bool,

    /// Comma-separated dotted JSON paths to project (e.g. milestone.spec_status). Unknown paths
    /// are a hard error. Applies to read commands: show, list, status, validate, reviews.
    #[arg(long, global = true, value_delimiter = ',')]
    pub fields: Vec<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Init {
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        from_repo: bool,
        /// M194: extends to the root `AGENTS.md` too. The
        /// combination `--force --merge-root-agents` is
        /// rejected by clap (mutually exclusive — destructive
        /// vs. additive intent).
        #[arg(long, conflicts_with = "merge_root_agents")]
        force: bool,
        /// M194: append the root-AGENTS snippet to an existing
        /// root `AGENTS.md` instead of warning. Mutually
        /// exclusive with `--force`.
        #[arg(long, conflicts_with = "force")]
        merge_root_agents: bool,
        #[arg(long)]
        with_cursor_skill: bool,
        #[arg(long)]
        with_opencode_skill: bool,
        #[arg(long)]
        skip_root_agents: bool,
        /// M194: rewrite `master-plan/AGENTS.md` from the current
        /// binary's embedded template. Skips confirmation when
        /// `--yes` is also passed. Scope is AGENTS.md only —
        /// `config.json` / `plan.json` drift is a separate
        /// doctor check (Q-02 resolution).
        #[arg(long)]
        refresh: bool,
        /// M194: skip the confirmation prompt for `--refresh`.
        #[arg(long, requires = "refresh")]
        yes: bool,
    },
    Install {
        #[arg(long, default_value = "opencode", value_delimiter = ',')]
        harness: Vec<String>,
        #[arg(long, short = 'g')]
        global: bool,
        #[arg(long)]
        dev: bool,
        #[arg(long)]
        source: Option<std::path::PathBuf>,
        #[arg(long)]
        print_paths: bool,
        #[arg(long)]
        toolkit_only: bool,
        /// Deploy only the listed skills (comma-separated). Omit to
        /// deploy the 3 base CPD skills (mp-flow, mp-runner,
        /// mp-coordinator). Pass `spec-grill` (alone or with the base
        /// set) to include the optional grill skill.
        #[arg(long, value_delimiter = ',')]
        skills: Vec<String>,
        /// M173 S2: deploy only the listed agents (comma-separated).
        /// Agents live at `templates/harness/<harness>/agents/<id>.md`
        /// and deploy to `<agent_profile_dir>/<id>.md`. Pass
        /// `mp-planner` (alone or alongside other ids) to deploy the
        /// dedicated planning agent. Omit to skip agent deploy.
        #[arg(long, value_delimiter = ',')]
        agents: Vec<String>,
        /// Validate the skill registry consistency without deploying.
        #[arg(long)]
        check: bool,
        /// M146: list the registry skills with deployment state per
        /// harness. Does not deploy.
        #[arg(long)]
        list_skills: bool,
    },
    Uninstall {
        #[arg(long, value_delimiter = ',')]
        harness: Vec<String>,
        #[arg(long, short = 'g')]
        global: bool,
        #[arg(long)]
        purge: bool,
    },
    Doctor {
        #[arg(long)]
        project: bool,
    },
    Specs {
        #[command(subcommand)]
        cmd: SpecsCmd,
    },
    /// Condensed spec-review surface (M80): review-oriented projection +
    /// since-last-approval spec diff.
    Spec {
        #[command(subcommand)]
        cmd: SpecCmd,
    },
    Annotation {
        #[command(subcommand)]
        cmd: AnnotationCmd,
    },
    /// M207: autopilot session/notes/transitions.
    Autopilot {
        #[command(subcommand)]
        cmd: AutopilotCmd,
    },
    Brownfield {
        #[command(subcommand)]
        cmd: BrownfieldCmd,
    },
    Brief {
        #[command(subcommand)]
        cmd: BriefCmd,
    },
    Idea {
        #[command(subcommand)]
        cmd: IdeaCmd,
    },
    Session {
        #[command(subcommand)]
        cmd: SessionCmd,
    },
    Agent {
        #[command(subcommand)]
        cmd: AgentCmd,
    },
    Skill {
        #[command(subcommand)]
        cmd: SkillCmd,
    },
    Graph {
        #[arg(long)]
        milestone: Option<String>,
        #[arg(long)]
        with_steps: bool,
        #[arg(long)]
        with_ac: bool,
        #[command(subcommand)]
        cmd: Option<GraphCmd>,
    },
    Inbox {
        /// Filter inbox items: actionable (default), all, spec-review, execution-review, review
        #[arg(long, default_value = "actionable")]
        filter: String,
    },
    Hygiene {
        #[arg(long, default_value_t = 30)]
        stale_days: u32,
    },
    /// Plan migrations. Legacy: `mp migrate --kinds [--dry-run]`.
    /// M177: `mp migrate manual-prefix-backfill [--dry-run] [--yes]`.
    Migrate {
        /// Optional subcommand (`manual-prefix-backfill`). When absent,
        /// `--kinds` selects the M102 kinds collapse.
        #[command(subcommand)]
        cmd: Option<MigrateCmd>,
        /// M102 R3: collapse tracks (BF-/TW-) and ideas into backlog.
        #[arg(long)]
        kinds: bool,
        #[arg(long)]
        dry_run: bool,
        /// Required to apply `manual-prefix-backfill` (and ignored for `--kinds`).
        #[arg(long)]
        yes: bool,
    },
    Backlog {
        #[command(subcommand)]
        cmd: BacklogCmd,
    },
    Decision {
        #[command(subcommand)]
        cmd: DecisionCmd,
    },
    Config {
        #[command(subcommand)]
        cmd: ConfigCmd,
    },
    Validate {
        /// Summary mode: ok/error counts + warnings grouped by code
        #[arg(long)]
        summary: bool,
    },
    Sync,
    Status {
        /// Summary mode: headline metrics only (no path block / suggested_path nesting)
        #[arg(long)]
        summary: bool,
    },
    /// M180: bounded read of the project activity journal.
    Activity(ActivityCmd),
    /// M180: consolidated project-health snapshot.
    Overview(OverviewCmd),
    Next {
        /// M102: lane selector — default = execution head; with --lane,
        /// returns that lane's head item. Affects which list the
        /// `head` projection reads from.
        #[arg(long, value_enum)]
        lane: Option<path::LaneArg>,
        /// Skip per-lane counts + effort rollup (default reads just the head).
        #[arg(long)]
        summary: bool,
    },
    Path {
        #[arg(long, default_value_t = 50)]
        horizon: usize,
        #[arg(long)]
        include_grooming: bool,
        #[arg(long)]
        prioritize_coverage: bool,
        #[arg(long)]
        include_coverage_gaps: bool,
        #[arg(long, value_enum)]
        lane: Option<path::LaneArg>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        no_ideas: bool,
        #[arg(long)]
        summary: bool,
        #[command(subcommand)]
        cmd: Option<PathSubCmd>,
    },
    Plan {
        #[command(subcommand)]
        cmd: PlanCmd,
    },
    Execution {
        #[command(subcommand)]
        cmd: ExecutionCmd,
    },
    Reviews {
        #[command(subcommand)]
        cmd: ReviewsCmd,
    },
    List {
        #[command(subcommand)]
        target: ListTarget,
    },
    Show {
        #[command(subcommand)]
        target: ShowTarget,
    },
    Interview {
        #[command(subcommand)]
        cmd: InterviewCmd,
    },
    Track {
        #[command(subcommand)]
        cmd: TrackCmd,
    },
    Release {
        #[command(subcommand)]
        cmd: ReleaseCmd,
    },
    Milestone {
        #[command(subcommand)]
        cmd: MilestoneCmd,
    },
    Changelog {
        #[command(subcommand)]
        cmd: ChangelogCmd,
    },
    Digest {
        #[arg(long)]
        since_handoff: bool,
        #[arg(long)]
        since: Option<String>,
        #[arg(long)]
        days: Option<u32>,
        #[arg(long)]
        markdown: bool,
        #[arg(long)]
        out: Option<std::path::PathBuf>,
    },
    /// Plan-shape mutations (M105 / B-41): bulk edits to milestone files
    /// that don't fit cleanly under `milestone add|update|…` because they
    /// touch many files at once. Currently only `strip-dropped-keys`.
    Edit {
        #[command(subcommand)]
        cmd: EditCmd,
    },
    Note {
        #[command(subcommand)]
        cmd: NoteCmd,
    },
    Search {
        query: String,
        #[arg(long = "type")]
        filter_type: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Embed the full matched fragment under `hit.object`. Default
        /// (`snippet`) returns the snippet-only response to keep payload
        /// small.
        #[arg(long, default_value = "snippet")]
        include: String,
        /// Group hits by their parent milestone in the response.
        #[arg(long)]
        group_by: Option<String>,
    },
    Git {
        #[command(subcommand)]
        cmd: GitCmd,
    },
    Scratch {
        #[command(subcommand)]
        cmd: ScratchCmd,
    },
    /// M173 S3: walk the clap Command tree and emit markdown tables for
    /// each command group under `docs/concepts/06 - Reference/generated/`.
    /// The generator covers description, usage, options, and
    /// subcommands; `<!-- mp:include <fragment> -->` markers in
    /// `MP-COMMANDS.md` / `AGENT-READINESS.md` resolve to one of those
    /// generated files at build time.
    Docgen {
        /// Output directory for the generated markdown bundle. Default:
        /// `<plan_dir>/../docs/concepts/06 - Reference/generated/` (i.e.
        /// `<project_root>/docs/concepts/06 - Reference/generated/`).
        #[arg(long)]
        out: Option<std::path::PathBuf>,
        /// Only emit a single named command group (e.g. `milestone`,
        /// `reviews`). Default emits every group.
        #[arg(long)]
        group: Option<String>,
    },
    /// M173 S4: write a hunk-compatible agent-context sidecar at the
    /// given path, listing the milestone's findings (optionally scoped
    /// to one `F-NN`) + comments as inline annotations. hunk loads the
    /// sidecar at startup via `hunk diff --agent-context <path>`; the
    /// file is not hot-reloaded.
    ///
    /// Singular `review` (vs the existing plural `reviews`) is the
    /// documented surface per the M173 spec. `--finding F-XX` filters
    /// to one finding; without it, every open finding on the milestone
    /// is exported.
    Review {
        #[command(subcommand)]
        cmd: ReviewCmd,
    },
    /// M229 / AC-01: breaking-release cleanup gate.
    ///
    /// `preflight` is a read-only check that returns
    /// `{ok: bool, blockers: [...], target_version, evidence_releases}`
    /// describing whether the recorded next-major target version and
    /// migration-window evidence (M208 + M219 shipped in at least one
    /// release) are in place. `apply` writes the audit marker; it
    /// refuses when the preflight does.
    BreakingRelease {
        #[command(subcommand)]
        cmd: BreakingReleaseCmd,
    },
}

/// M229 / AC-01: subcommands under `mp breaking-release`.
#[derive(Subcommand, Debug)]
pub enum BreakingReleaseCmd {
    /// AC-01: read-only gate check. Returns `{ok, blockers[], target_version, evidence_releases}`.
    /// Refuses removal work unless both the next-major target version and the
    /// migration-window evidence are recorded.
    Preflight,
    /// AC-01: write the audit marker (`<plan_dir>/.mp/breaking_release.json`).
    /// Refuses to apply unless the preflight is green.
    Apply,
}

/// Subcommands under `mp migrate`.
#[derive(Subcommand, Debug)]
pub enum MigrateCmd {
    /// M177 S3: prefix prose AC verifications with `manual: `.
    ///
    /// Dry-run (default without `--yes`) previews hits; `--yes` applies.
    /// `--dry-run` wins over `--yes` (never writes when both are set).
    /// Idempotent: re-running after a clean apply is a no-op.
    ManualPrefixBackfill {
        /// Preview without writing.
        #[arg(long)]
        dry_run: bool,
        /// Required to actually write.
        #[arg(long)]
        yes: bool,
    },
}

/// Subcommands under `mp review` (singular). M173 S4 surface.
#[derive(Subcommand, Debug)]
pub enum ReviewCmd {
    /// Write a hunk-compatible agent-context sidecar at `--output`.
    /// Loads via `hunk diff --agent-context <path>`; not hot-reloaded.
    /// `--finding F-XX` filters to one finding; default exports every
    /// finding on the milestone.
    Sidecar {
        milestone: String,
        /// Filter to one finding id (e.g. `F-01`).
        #[arg(long)]
        finding: Option<String>,
        /// Path to write the sidecar JSON to. Required.
        #[arg(long)]
        output: std::path::PathBuf,
    },
}

pub fn resolve_format(cli_format: Option<OutputFormat>) -> OutputFormat {
    if let Some(f) = cli_format {
        return f;
    }
    OutputFormat::Json
}
