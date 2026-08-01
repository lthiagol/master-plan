use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum ReviewsCmd {
    /// Unified review discovery: execution-review queue + spec-review milestones
    Status,
    Pending {
        /// Group pending reviews by field (e.g. completed_at for year-month groups)
        #[arg(long)]
        group_by: Option<String>,
        /// Include per-item summary (steps done/total, open findings)
        #[arg(long)]
        summary: bool,
        /// Filter pending reviews by preset (force-bypassed, etc.) — same presets accepted as `pass --all --filter`
        #[arg(long)]
        filter: Option<String>,
    },
    Pass {
        /// Milestone ID (required unless --all is set)
        milestone: Option<String>,
        #[arg(long)]
        verdict: String,
        #[arg(long)]
        reviewer: String,
        #[arg(long)]
        notes: Option<String>,
        /// Resolve all pending reviews in one batch
        #[arg(long)]
        all: bool,
        /// Filter pending reviews by preset (force-bypassed, etc.) when --all is used
        #[arg(long)]
        filter: Option<String>,
    },
    List,
    Show {
        milestone: String,
    },
    /// Classify the pending review queue into risk buckets (alias: triage)
    Sweep,
    /// Cross-project rollup of milestones by review_state
    Lifecycle {
        /// Summary mode: return only the bucket counts [{review_state, count}]
        #[arg(long)]
        summary: bool,
    },
    /// Manage structured findings on milestones
    #[command(subcommand)]
    Finding(FindingCmd),
    /// M133 AC-01: add or list threaded review comments on a milestone
    #[command(subcommand)]
    Comment(CommentCmd),
    /// M133 AC-02: record a coordinator/runner hand-off on a milestone.
    /// The persisted shape mirrors the hand-off protocol documented in
    /// `mp-flow`'s Hand-off protocol section (from/to direction, data,
    /// session-boundary, evidence).
    ///
    /// M142 AC-07 + AC-08: env-var auto-injection contract.
    ///
    /// `MP_SESSION_ID` — stable string the harness sets per session
    /// (opencode conversation id, pi UUID, etc.). Pre-populates
    /// `from_session` only when `--from-session` is absent. Does NOT
    /// fill `to_session` (that would create a same-session cross-role
    /// hand-off); pass `--to-session` explicitly for the receiving side.
    ///
    /// `MP_SESSION_ROLE` — `coordinator` or `runner`. Matches `mp agent
    /// role`. Pre-populates `from_role`; `to_role` is set to the
    /// complement (coordinator→runner, runner→coordinator).
    ///
    /// Per-field override semantics: each manual flag overrides the
    /// env value for the field it sets. `--from-session manual-sess`
    /// overrides `from_session`; `to_session` stays empty unless
    /// `--to-session` is given. Same for `--from-role` / `--to-role`
    /// and `MP_SESSION_ROLE`. The contract is forgeable-by-humans and
    /// auditable-by-review — real L5 enforcement requires harness
    /// trust, which is a separate design discussion. This milestone
    /// is the audit trail, not the enforcement.
    Handoff {
        /// Milestone ID
        milestone: String,
        /// Producing side session id (harness conversation / UUID)
        #[arg(long)]
        from_session: Option<String>,
        /// Receiving side session id (must differ at cross-role hand-offs)
        #[arg(long)]
        to_session: Option<String>,
        /// M142: structured role of the producing side (`coordinator` |
        /// `runner`). Distinct from `from_session` (the harness's
        /// session id). Pre-populated by MP_SESSION_ROLE.
        #[arg(long)]
        from_role: Option<String>,
        /// M142: structured role of the receiving side. Pre-populated
        /// as the complement of `from_role` when MP_SESSION_ROLE is set.
        #[arg(long)]
        to_role: Option<String>,
        /// What data passes at this hand-off point (free-form; the
        /// hand-off protocol recommends a structured shape but the
        /// CLI does not enforce one)
        #[arg(long)]
        data: String,
        /// Session-boundary note (e.g. "coordinator's planning session
        /// closes; runner's execution session opens in a fresh session")
        #[arg(long)]
        session_boundary: Option<String>,
        /// Evidence the producing side leaves behind (registry entries,
        /// milestone file state, commit chain, etc.)
        #[arg(long)]
        evidence: Option<String>,
        /// RFC3339 timestamp override; defaults to now
        #[arg(long)]
        at: Option<String>,
    },
    /// M142 AC-01..AC-05: run the L5 evidence audit on a milestone's
    /// hand-off records. Detects three violation classes:
    /// `same_session_across_role_boundary`,
    /// `missing_session_identity`, `role_inversion`. Output is JSON:
    /// `{ok, violations, summary}`. Exit code is 0 in both clean and
    /// violation cases (advisory, not blocking).
    L5Check {
        /// Milestone ID
        milestone_id: String,
    },
    /// M154: export the milestone's findings + comments as hunk-
    /// compatible JSON. The default channel is the live `comment
    /// apply` batch on stdout (pipe to `hunk session comment apply
    /// --stdin`). `--file <path>` switches to the agent-context
    /// sidecar (loaded at startup by `hunk diff --agent-context
    /// <path>`). `--apply` pipes the batch into a live session when
    /// one is running; without a live session it prints the batch
    /// and a hint instead of erroring (per AC-04).
    Hunk {
        /// Milestone ID
        milestone: String,
        /// Write the agent-context sidecar to this path instead of
        /// the live batch to stdout. hunk loads the sidecar at
        /// startup; the live batch is for piping into a running
        /// session. Mutually exclusive with stdout-only output —
        /// when set, stdout is silenced.
        #[arg(long, value_name = "PATH")]
        file: Option<String>,
        /// Pipe the live batch into a running `hunk session` when one
        /// is live; otherwise print the batch + hint and exit 0.
        /// `--apply` is a no-op alongside `--file` (the sidecar is
        /// not piped).
        #[arg(long)]
        apply: bool,
        /// Drop unanchored findings / comments (those without a
        /// file path). Default off — unanchored notes surface as
        /// file-level summary entries so the human review still
        /// sees the milestone-level feedback. `--strict` is
        /// available when the consumer wants only line-anchored
        /// annotations (e.g. a downstream filter that re-emits a
        /// clean diff).
        #[arg(long)]
        strict: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum CommentCmd {
    /// Add a threaded review comment on a milestone
    Add {
        /// Milestone ID
        milestone: String,
        /// Comment author (e.g. "reviewer", "agent:coordinator")
        #[arg(long)]
        author: String,
        /// Comment body (non-empty, one or more sentences)
        #[arg(long)]
        body: String,
        /// Optional finding link (`F-NN`) so the comment anchors on a
        /// specific finding
        #[arg(long)]
        finding: Option<String>,
        /// RFC3339 timestamp override; defaults to now
        #[arg(long)]
        at: Option<String>,
        /// M154 AC-02: file path for the comment location. Absent
        /// location preserves the current milestone-anchored behavior
        /// (no migration; backward compatible — see AC-02 in the
        /// spec). Combines with `--line` / `--side` to attach the
        /// comment to a code location for hunk export.
        #[arg(long, value_name = "PATH")]
        file: Option<String>,
        /// M154 AC-02: 1-based line number for the comment location.
        /// Pairs with `--file`; sets `new_range.start_line` or
        /// `old_range.start_line` based on `--side`.
        #[arg(long, value_name = "N")]
        line: Option<u32>,
        /// M154 AC-02: which side of a diff the line refers to. One of
        /// `old` or `new`; defaults to `new` when `--line` is given
        /// without `--side`. Without `--line`, `--side` is a no-op.
        #[arg(long, value_name = "old|new")]
        side: Option<String>,
    },
    /// List threaded review comments on a milestone, oldest-first
    List {
        /// Milestone ID
        milestone: String,
    },
}

#[derive(Subcommand, Debug)]
// `FindingCmd::Add` carries ~14 string/option fields (M101 + M154
// --file/--line/--side additions) which makes the enum >300 bytes.
// The clippy::large_enum_variant warning is structural — every
// variant of a clap Subcommand already lives on the heap via
// Box<dyn ArgsParse> internally, so the size diff doesn't translate
// to a runtime cost. Suppressing the lint here keeps the public CLI
// shape flat without `Box<Option<String>>` noise on every field.
#[allow(clippy::large_enum_variant)]
pub enum FindingCmd {
    /// Add a finding to a milestone
    Add {
        /// Milestone ID
        milestone: String,
        #[arg(long)]
        severity: String,
        #[arg(long)]
        category: String,
        #[arg(long)]
        desc: String,
        #[arg(long)]
        author: Option<String>,
        /// M101: review phase that owns this finding. Empty string
        /// (default) preserves the legacy behavior — finding treated as
        /// self-phase for gating purposes per the M125 convention.
        #[arg(long, allow_hyphen_values = true)]
        phase: Option<String>,
        /// M101: anchor for hunk-compatible export. Format:
        /// `path:commit:new_range:old_range:hunk_index:side` where
        /// `new_range` / `old_range` are `START-END` line numbers
        /// (e.g., `10-20`) and `side` is `old` or `new`. Most fields
        /// are optional; missing segments parse to empty/None.
        #[arg(long, allow_hyphen_values = true)]
        anchor: Option<String>,
        /// M154 AC-02: file path for the finding location (alternative
        /// to the heavier `--anchor path:...` form). Implies the
        /// finding is file-anchored; combine with `--line` and
        /// `--side` to produce the same shape as `--anchor` would.
        /// When both `--file` and `--anchor` are present, `--anchor`
        /// wins (the explicit-positional form is canonical).
        #[arg(long, value_name = "PATH")]
        file: Option<String>,
        /// M154 AC-02: 1-based line number for the finding location.
        /// Sets `new_range.start_line` (default side=new) or
        /// `old_range.start_line` when `--side old` is given. The
        /// end_line is the same as start_line (single-line location).
        #[arg(long, value_name = "N")]
        line: Option<u32>,
        /// M154 AC-02: which side of a diff the line refers to. One of
        /// `old` or `new`; defaults to `new` when `--line` is given
        /// without `--side`. Without `--line`, `--side` is a no-op.
        #[arg(long, value_name = "old|new")]
        side: Option<String>,
        /// M101: one-line summary (hunk AgentAnnotation shape).
        #[arg(long)]
        summary: Option<String>,
        /// M101: rationale (hunk AgentAnnotation shape).
        #[arg(long)]
        rationale: Option<String>,
        /// M101: reviewer confidence. One of low, medium, high, or empty.
        #[arg(long, allow_hyphen_values = true)]
        confidence: Option<String>,
        /// M101: comma-separated tags.
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
    },
    /// Resolve (fix) a finding on a milestone
    Resolve {
        /// Milestone ID
        milestone: String,
        /// Finding ID (e.g. F-01); omit with --all
        finding: Option<String>,
        /// Resolve every open finding on the milestone
        #[arg(long)]
        all: bool,
        #[arg(long)]
        commit: Option<String>,
    },
    /// List findings for a milestone
    List {
        /// Milestone ID
        milestone: String,
        /// Only show open findings
        #[arg(long)]
        open: bool,
        /// Summary mode: return only {open, fixed, total} counts
        #[arg(long)]
        summary: bool,
    },
}
