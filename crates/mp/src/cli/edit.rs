use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum EditCmd {
    /// M105 S4 (B-41): one-shot utility that strips every key in
    /// `milestone::DROPPED_CEREMONY_KEYS` from every milestone file in the
    /// plan. Idempotent — re-running on a clean plan is a no-op (no file
    /// is rewritten when no key matches). Pair with `mp validate --summary`
    /// to confirm post-run counts are 0/0.
    StripDroppedKeys,
    /// M100 S10: bulk-migrate every milestone in the plan from the legacy
    /// `spec_status` + `execution_status` shape to the unified `lifecycle`
    /// field. Idempotent — re-running on an already-migrated plan is a no-op.
    /// Use `--dry-run` first to preview; `--yes` is required to actually
    /// write. After running, `mp validate` should exit 0; if it doesn't,
    /// the migration surfaces new gate failures (rare; lifecycle value may
    /// differ from legacy-derive) — review and re-validate.
    MigrateLifecycle {
        /// Preview the change set without writing any files. Emits
        /// `{milestone: <id>, before: <lifecycle>, after: <lifecycle>}`
        /// for each milestone that would change.
        #[arg(long)]
        dry_run: bool,
        /// Required to actually write. Without `--yes` the command only
        /// prints what would change. Defensive double-gate alongside
        /// `--dry-run`: a destructive op must require an explicit ack.
        #[arg(long)]
        yes: bool,
    },
    /// M177 S8: clear stale `deferred_reason` text where `deferred: false`.
    ///
    /// When a milestone was deferred then reopened/set in-progress, the
    /// rationale text was retained because no command cleared it. Idempotent.
    /// Use `--dry-run` to preview; `--yes` is required to write.
    StripDeferredReason {
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
    },
}
