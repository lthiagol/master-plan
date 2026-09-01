use super::parsers::files_value_parser;
use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum MilestoneCmd {
    Create {
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        file: Option<std::path::PathBuf>,
        /// JSON input (inline, @file, or @- for stdin). See MP-COMMANDS.md for the accepted JSON shape.
        #[arg(long)]
        json: Option<String>,
        #[arg(long)]
        from_handoff: Option<String>,
        /// Print a schema-valid JSON template and exit
        #[arg(long)]
        example: bool,
    },
    Approve {
        id: String,
        /// M113 S2: compute and print the change set without writing.
        /// Exits 0 with `{dry_run: true, files: [...], fields: {...}}`.
        #[arg(long)]
        dry_run: bool,
    },
    Update {
        id: String,
        #[arg(long)]
        file: Option<std::path::PathBuf>,
        #[arg(long)]
        json: Option<String>,
        #[arg(long)]
        if_updated: Option<String>,
        /// Escape hatch: allow `acceptance_criteria` / `steps` arrays in the
        /// update JSON. M93 makes these default-rejected to prevent agents
        /// from rebuilding document arrays. Migration scripts only.
        #[arg(long)]
        replace_arrays: bool,
        /// M111 S5 escape hatch (parity with `--replace-arrays`): silently
        /// ignore fields that aren't part of the `update` JSON schema
        /// (`design_decisions`, `findings`, `verification`, `milestone`,
        /// etc.) instead of returning `unsupported field(s)`. Enables a
        /// round-trip `mp show milestone --format raw → mp milestone update
        /// --json` without manual `jq del(...)` stripping.
        #[arg(long)]
        accept_extra_fields: bool,
        /// M165: rewrite the milestone-level `verification.evidence`
        /// string. Reachable on lifecycle=complete milestones; useful for
        /// flipping the `[force-bypassed` marker once a follow-up milestone
        /// closes the debt, or for stamping a post-completion remediation
        /// tombstone. Absent flag preserves the existing value.
        #[arg(long)]
        verification: Option<String>,
        /// M165: same surface as `--verification`, but reads the evidence
        /// text from a file path. Preferred for long evidence values that
        /// would otherwise need shell escaping. Empty file is rejected.
        #[arg(long)]
        verification_file: Option<std::path::PathBuf>,
        /// M165: set the `verification.date` companion field
        /// (`YYYY-MM-DD`). Independent of `--verification`.
        #[arg(long)]
        verification_date: Option<String>,
        /// M165: set the `verification.branch` companion field (branch
        /// name). Independent of `--verification`.
        #[arg(long)]
        verification_branch: Option<String>,
    },
    SetSpecStatus {
        id: String,
        status: String,
    },
    /// Set the canonical lifecycle field directly (post-M100).
    /// Mutates `lifecycle` and derives the `spec_status` /
    /// `execution_status` legacy aliases so on-disk files stay
    /// internally consistent. Use `--dry-run` to preview; pass `""`
    /// to reset to a blank lifecycle.
    SetLifecycle {
        id: String,
        /// New lifecycle value (one of: draft, groomed, approved,
        /// in-progress, done, self-reviewed, reviewed, complete,
        /// remediation) or `""` to clear.
        status: String,
        /// Compute and print the change set without writing.
        #[arg(long)]
        dry_run: bool,
    },
    SetPriority {
        id: String,
        priority: String,
    },
    SetStatus {
        id: String,
        status: String,
        /// M113 S2: compute and print the change set without writing.
        /// Exits 0 with `{dry_run: true, files: [...], fields: {...}}`.
        #[arg(long)]
        dry_run: bool,
    },
    /// Set the target release version for a milestone
    SetTargetVersion {
        id: String,
        version: String,
    },
    Complete {
        id: String,
        #[arg(long)]
        evidence: Option<String>,
        #[arg(long)]
        evidence_file: Option<std::path::PathBuf>,
        #[arg(
            long,
            help = "Bypass the AC verification gate (records the bypass in evidence). \
                    Does NOT bypass the review gate (use --skip-review for that)."
        )]
        force: bool,
        #[arg(
            long,
            help = "Skip both AC and step verifications entirely (records `[skip-verify]` in evidence). \
                    Use for re-runs after a known-good manual smoke, CI gating, or follow-up completions \
                    where verifications are intentionally out of scope. Stronger than --force."
        )]
        skip_verify: bool,
        /// M196: bypass the review gate — non-track milestones reach
        /// terminal `complete` without an independent review. Records
        /// `[skip-review: ...]` in evidence as recorded debt. Use only
        /// for rare exceptions where the debt is acceptable; the
        /// default is `false` (no bypass).
        #[arg(
            long,
            help = "Bypass the review gate — non-track milestones reach terminal `complete` \
                    without an independent review. Records `[skip-review: ...]` in evidence as \
                    recorded debt. Default: false. Use --force / --skip-verify for the AC \
                    verification gate, not this one."
        )]
        skip_review: bool,
        #[arg(long)]
        executor: Option<String>,
        /// M113 S2: compute and print the change set (files, flipped fields,
        /// AC + step verifications that would run) without writing.
        #[arg(long)]
        dry_run: bool,
    },
    Verify {
        id: String,
    },
    Criterion {
        #[command(subcommand)]
        cmd: CriterionCmd,
    },
    /// Agent-friendly short alias for `criterion` (M93). Lets agents say
    /// `mp milestone ac show 87 AC-03` without loading the whole document.
    Ac {
        #[command(subcommand)]
        cmd: CriterionCmd,
    },
    Decompose {
        id: String,
        #[arg(long)]
        work_packages: Option<u32>,
    },
    Plan {
        id: String,
        #[arg(long)]
        work_packages: Option<u32>,
    },
    Block {
        id: String,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        by: Option<String>,
    },
    Unblock {
        id: String,
    },
    Defer {
        id: String,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        by: Option<String>,
    },
    Reopen {
        id: String,
    },
    Split {
        id: String,
        #[arg(long, default_value_t = 2)]
        into: u32,
        #[arg(long, value_delimiter = ',')]
        titles: Option<Vec<String>>,
    },
    Question {
        #[command(subcommand)]
        cmd: QuestionCmd,
    },
    Delete {
        id: String,
        #[arg(long)]
        force: bool,
    },
    Archive {
        id: String,
    },
    Restore {
        id: String,
    },
    Purge {
        id: String,
    },
    Groom {
        id: String,
    },
    Trace {
        id: String,
    },
    /// M112 S4: read-only milestone history log. Emits created/updated from
    /// the on-disk milestone plus a `commits` array of `git log --oneline`
    /// for the milestone file. No on-disk writes.
    Log {
        id: String,
    },
    /// List milestones that depend on `<ID>` (reverse deps)
    Dependents {
        id: String,
    },
    /// List milestones that `<ID>` depends on (forward deps)
    Deps {
        id: String,
    },
    /// Transitive blast radius: recursive reverse deps + path pins + ordering implications
    Impact {
        id: String,
    },
    /// List milestones pending code review (spec_status=implemented, code_review=true)
    ListPendingReview,
    #[command(subcommand)]
    Challenge(ChallengeCmd),
    #[command(subcommand)]
    Step(StepCmd),
    #[command(subcommand)]
    Wp(WpCmd),
    #[command(subcommand)]
    DesignDecision(DesignDecisionCmd),
    /// Bulk milestone metadata operations (M94). Targets resolve via --ids
    /// and/or --where (same filter syntax as `list milestones`). Sequential
    /// execution with per-id result reporting; --dry-run previews mutations.
    #[command(subcommand)]
    Bulk(BulkCmd),
    /// M202: per-stage mp-flow tracker. Read all 12 stages as a CLI table
    /// (`mp milestone stage list <id>`), or set one stage explicitly
    /// (`mp milestone stage set <id> <stage> <status>`). Explicit sets
    /// override auto-derive — a stage the user flipped to `done` stays
    /// `done` even when a subsequent lifecycle transition would normally
    /// re-write it. The only escape from the override is another
    /// explicit `set` (or a fresh lifecycle event after the override
    /// was cleared). Hand-off is included in the 12-stage list but
    /// ONLY advances via this CLI (AC-11).
    Stage {
        #[command(subcommand)]
        cmd: StageCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum StageCmd {
    /// List all 12 mp-flow stages for a milestone as a CLI table
    /// (id, status, at). Stages without an entry yet show `pending`
    /// and `—` for the timestamp. Exit 0 even when the milestone has
    /// no recorded flow_stages (the table is canonical regardless of
    /// how many stages have actually fired).
    List { id: String },
    /// Set one stage to a new status. Accepts only the canonical
    /// 12 stage slugs and the 4-value status enum (`pending`,
    /// `done`, `in_progress`, `skipped`). Exit 2 on invalid input;
    /// the milestone file is unchanged on rejection.
    Set {
        id: String,
        stage: String,
        status: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum BulkCmd {
    /// Bulk update `priority` for one or more milestones.
    SetPriority {
        /// Comma-separated milestone ids to update (e.g. 82,92,93).
        #[arg(long, value_delimiter = ',')]
        ids: Option<Vec<String>>,
        /// Repeatable filter expressions (e.g. `spec_status==review`).
        #[arg(long = "where", value_delimiter = ',')]
        r#where: Vec<String>,
        /// Priority value (urgent|high|normal|low).
        #[arg(long)]
        priority: String,
        /// Resolve targets and report planned mutations without writing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Bulk update `spec_status` for one or more milestones.
    SetSpecStatus {
        #[arg(long, value_delimiter = ',')]
        ids: Option<Vec<String>>,
        #[arg(long = "where", value_delimiter = ',')]
        r#where: Vec<String>,
        /// Spec status value (draft|review|ready|verified|implemented).
        #[arg(long)]
        status: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// Bulk update the canonical `lifecycle` field (post-M100).
    /// Pass `""` to clear the lifecycle; this is the migration
    /// re-derivation path. The `spec_status` / `execution_status`
    /// legacy aliases follow the canonical value automatically.
    SetLifecycle {
        #[arg(long, value_delimiter = ',')]
        ids: Option<Vec<String>>,
        #[arg(long = "where", value_delimiter = ',')]
        r#where: Vec<String>,
        /// Lifecycle value (draft, groomed, approved, in-progress,
        /// done, self-reviewed, reviewed, complete, remediation) or
        /// empty to reset.
        #[arg(long)]
        status: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// M202: bulk-update one mp-flow stage across multiple milestones.
    /// Mirrors bulk set-priority: targets resolve via --ids / --where,
    /// cancelled milestones are skipped (no-op) and listed with reason
    /// `cancelled` in the per-id report, --dry-run previews mutations,
    /// exit code 2 on partial failure. The `--stage` slug must be one
    /// of the canonical 12 (`draft` … `hand-off`); `--status` must be
    /// one of `pending | done | in_progress | skipped`. Both are
    /// validated upfront so a typo aborts the whole batch with a
    /// precise error instead of silently no-oping per target.
    SetStage {
        #[arg(long, value_delimiter = ',')]
        ids: Option<Vec<String>>,
        #[arg(long = "where", value_delimiter = ',')]
        r#where: Vec<String>,
        /// Stage slug (one of the canonical 12 mp-flow stage keys).
        #[arg(long)]
        stage: String,
        /// Stage status (pending | done | in_progress | skipped).
        #[arg(long)]
        status: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// Bulk add or remove a `depends_on` entry across milestones. Refuses to
    /// introduce cycles; per-id errors report the offending milestone.
    DependsOn {
        #[command(subcommand)]
        action: BulkDependsOnAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum BulkDependsOnAction {
    Add {
        #[arg(long, value_delimiter = ',')]
        ids: Option<Vec<String>>,
        #[arg(long = "where", value_delimiter = ',')]
        r#where: Vec<String>,
        /// Milestone id to add as a dependency (e.g. 87).
        #[arg(long)]
        depends_on: String,
        #[arg(long)]
        dry_run: bool,
    },
    Remove {
        #[arg(long, value_delimiter = ',')]
        ids: Option<Vec<String>>,
        #[arg(long = "where", value_delimiter = ',')]
        r#where: Vec<String>,
        #[arg(long)]
        depends_on: String,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum QuestionCmd {
    Add {
        id: String,
        #[arg(long)]
        text: String,
    },
    Resolve {
        id: String,
        qid: String,
        #[arg(long)]
        resolution: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum CriterionCmd {
    Pass {
        id: String,
        ac_id: String,
        #[arg(long)]
        evidence: Option<String>,
    },
    Fail {
        id: String,
        ac_id: String,
        #[arg(long)]
        reason: Option<String>,
    },
    Add {
        id: String,
        #[arg(long)]
        description: String,
        #[arg(long)]
        verification: String,
    },
    /// Show one acceptance criterion as a single-fragment JSON object
    /// (id, description, verification, status, evidence) — agent-friendly read.
    Show { id: String, ac_id: String },
    /// List all acceptance criteria for a milestone as an array of single-fragment JSON objects.
    List { id: String },
    /// Update one acceptance criterion in place. Returns only the changed fragment.
    Update {
        id: String,
        ac_id: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        verification: Option<String>,
        /// M111 S1: write the fragment's `evidence` field in place. Lets agents
        /// stamp per-AC run evidence without falling back to
        /// `mp milestone update --json --replace-arrays`.
        #[arg(long)]
        evidence: Option<String>,
    },
    /// M118 S1: bulk-update many acceptance criteria in one invocation.
    /// Takes a JSON array of `{id, description?, verification?, evidence?}`
    /// fragment updates applied through the same per-AC flow as `Update`.
    /// Empty array is a no-op. Missing id fails fast with a per-id error.
    /// Path-mode only; stdin-mode (`@-`) deferred per design_decisions.
    Bulk {
        id: String,
        /// Path to a JSON file containing an array of fragment updates.
        #[arg(long, value_name = "FILE")]
        bulk: std::path::PathBuf,
    },
    /// Remove one acceptance criterion. Fails with a structured guard error
    /// when any step `covers_ac` includes this AC id.
    Remove { id: String, ac_id: String },
}

#[derive(Subcommand, Debug)]
pub enum StepCmd {
    Add {
        milestone: String,
        #[arg(long)]
        wp: String,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        after: Option<String>,
        #[arg(long)]
        action: Option<String>,
        /// Value format: bare path (`crates/mp/src/main.rs`) or
        /// comma-separated list (`--files a.rs,b.rs`). Quoted JSON
        /// arrays (e.g. `--files '["a.rs"]'`) and object literals are
        /// rejected by the value parser — pass bare paths or a
        /// comma-separated list instead. M116 AC-02 docs follow-up.
        #[arg(long, value_parser = files_value_parser)]
        files: Option<String>,
        #[arg(long)]
        tests: Option<String>,
        #[arg(long)]
        done_when: Option<String>,
        #[arg(long)]
        covers_ac: Option<String>,
    },
    SetStatus {
        milestone: String,
        step: String,
        status: String,
    },
    /// Show one step as a single-fragment JSON object
    /// (id, action, done_when, tests, covers_ac, work_package, status, …).
    /// Agent-friendly read: no full milestone document.
    Show {
        milestone: String,
        step: String,
    },
    Done {
        milestone: String,
        step: String,
    },
    Update {
        milestone: String,
        step: String,
        #[arg(long)]
        action: Option<String>,
        /// Value format: bare path (`crates/mp/src/main.rs`) or
        /// comma-separated list (`--files a.rs,b.rs`). Quoted JSON
        /// arrays (e.g. `--files '["a.rs"]'`) and object literals are
        /// rejected by the value parser — pass bare paths or a
        /// comma-separated list instead. M116 AC-02 docs follow-up.
        #[arg(long, value_parser = files_value_parser)]
        files: Option<String>,
        #[arg(long)]
        tests: Option<String>,
        #[arg(long)]
        done_when: Option<String>,
        #[arg(long)]
        covers_ac: Option<String>,
        #[arg(long)]
        wp: Option<String>,
        #[arg(long)]
        depends_on_steps: Option<String>,
        /// M111 S1: write the step's `evidence` field in place, mirroring the
        /// `mp milestone ac update --evidence` flag.
        #[arg(long)]
        evidence: Option<String>,
    },
    Split {
        milestone: String,
        step: String,
    },
    /// Remove a step. Fails when another step's `depends_on_steps` includes
    /// the target, or when its id is a parent of split children. Returns
    /// `{ ok, removed: "<step-id>" }` on success.
    Remove {
        milestone: String,
        step: String,
    },
    Fail {
        milestone: String,
        step: String,
    },
    Claim {
        milestone: String,
        step: String,
        #[arg(long)]
        by: String,
        #[arg(long)]
        lease: Option<String>,
    },
    Release {
        milestone: String,
        step: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum WpCmd {
    Add {
        milestone: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        goal: Option<String>,
        #[arg(long)]
        rollback: Option<String>,
        #[arg(long)]
        id: Option<String>,
    },
    Update {
        milestone: String,
        wp: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        goal: Option<String>,
        #[arg(long)]
        rollback: Option<String>,
    },
    /// Remove a work package. Fails when any step still references it via
    /// `work_package`. Returns `{ ok, removed: "<wp-id>" }` on success.
    Remove { milestone: String, wp: String },
}

#[derive(Subcommand, Debug)]
pub enum DesignDecisionCmd {
    /// Add a design decision to a milestone. `--area` is required; the schema
    /// marks `area` non-empty and the CLI now exposes it (M111 S4).
    Add {
        id: String,
        #[arg(long)]
        area: String,
        #[arg(long)]
        decision: String,
        #[arg(long)]
        rationale: String,
    },
    /// Update one design decision in place. Either `--index <N>` or
    /// `--area <TEXT>` selects the target (first match wins for `--area`).
    /// Only the supplied fields are mutated.
    Update {
        id: String,
        #[arg(long, conflicts_with = "area")]
        index: Option<usize>,
        #[arg(long, conflicts_with = "index")]
        area: Option<String>,
        #[arg(long)]
        new_area: Option<String>,
        #[arg(long)]
        decision: Option<String>,
        #[arg(long)]
        rationale: Option<String>,
    },
    /// Remove one design decision. Either `--index <N>` or `--area <TEXT>`
    /// selects the target.
    Remove {
        id: String,
        #[arg(long, conflicts_with = "area")]
        index: Option<usize>,
        #[arg(long, conflicts_with = "index")]
        area: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ChallengeCmd {
    Start {
        id: Option<String>,
        #[arg(long, default_value = "plan")]
        scope: String,
    },
    Audit {
        id: String,
        #[arg(long)]
        scope: Option<String>,
    },
    List {
        id: Option<String>,
        #[arg(long)]
        status: Option<String>,
    },
    Add {
        id: String,
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "major")]
        severity: String,
        #[arg(long, default_value = "gap")]
        category: String,
        #[arg(long)]
        target: Option<String>,
        #[arg(long)]
        description: Option<String>,
    },
    Resolve {
        id: String,
        finding_id: String,
        #[arg(long)]
        action: String,
        #[arg(long)]
        payload: Option<String>,
        #[arg(long)]
        resolution: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    Dismiss {
        id: String,
        finding_id: String,
        #[arg(long)]
        reason: String,
    },
    Done {
        id: String,
    },
}
