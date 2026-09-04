//! M153 S2 MEDIUM-1: end-to-end test for `prompt_source` log emission.
//!
//! The S2 done_when says "the log records 'override' vs 'default' per
//! stage". This file pins that contract: a real `DriveLogger` attached
//! to a `DriveOps` receives a `prompt_source` event whenever the watch
//! loop calls `build_prompt_with`, and the event's `message` field
//! distinguishes the surface (default vs override) so an operator can
//! read the log to confirm which template served each stage.
//!
//! Without this test the contract was only exercisable via the in-
//! module `Scripted` mock (which no-ops `log_event`) or by manually
//! reading the JSONL output of an `mp watch` session.

mod common;

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use mp::autopilot::drive::{
    drive_milestone, BuildPromptRequest, DriveLogEntry, DriveLogger, DriveOps, DriveOutcome,
    LifecycleTarget, PaneHandle, PromptStage, Role, WaitOutcome,
};
use mp::model::{MilestoneFile, MilestoneMeta};

fn ms(id: &str, lifecycle: &str) -> MilestoneFile {
    MilestoneFile {
        milestone: MilestoneMeta {
            id: id.to_string(),
            lifecycle: lifecycle.to_string(),
            spec_status: "ready".to_string(),
            execution_status: if lifecycle == "complete" {
                "done".to_string()
            } else {
                "planned".to_string()
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Test-only `DriveOps` that forwards `log_event` to a real
/// `DriveLogger` opened on a tempdir file. Backed by a script of
/// canned milestones (the same pattern as `crates/mp/src/autopilot/drive/
/// state_machine.rs::Scripted`) so we can drive `drive_milestone`
/// end-to-end without spawning subprocesses.
struct LoggedScripted {
    milestones: RefCell<Vec<MilestoneFile>>,
    prompts_sent: RefCell<Vec<String>>,
    panes_ensured: RefCell<Vec<Role>>,
    handoffs: RefCell<Vec<String>>,
    plan_dir: PathBuf,
    logger: DriveLogger,
}

impl LoggedScripted {
    fn new(seq: Vec<MilestoneFile>, plan_dir: PathBuf, logger: DriveLogger) -> Self {
        Self {
            milestones: RefCell::new(seq),
            prompts_sent: RefCell::new(vec![]),
            panes_ensured: RefCell::new(vec![]),
            handoffs: RefCell::new(vec![]),
            plan_dir,
            logger,
        }
    }

    fn milestones_log_path(&self) -> PathBuf {
        self.logger.path()
    }
}

impl DriveOps for LoggedScripted {
    fn read_milestone(&mut self) -> anyhow::Result<MilestoneFile> {
        Ok(self
            .milestones
            .borrow()
            .first()
            .cloned()
            .unwrap_or_else(|| ms("1", "complete")))
    }
    fn ensure_pane(&mut self, role: Role) -> anyhow::Result<PaneHandle> {
        self.panes_ensured.borrow_mut().push(role);
        Ok(PaneHandle {
            label: format!("role-{}-1", role.label()),
            pane_id: format!("role-{}-1", role.label()),
            reused: false,
        })
    }
    fn send_prompt_to(&mut self, _pane: &PaneHandle, text: &str) -> anyhow::Result<()> {
        self.prompts_sent.borrow_mut().push(text.to_string());
        Ok(())
    }
    fn log_event(&self, kind: &'static str, message: impl Into<String>) {
        // Forward to the real DriveLogger. Tests assert on the JSONL
        // output of this logger rather than on the in-process
        // scripts — the integration value is "did the JSONL get
        // written at all?"
        let entry = DriveLogEntry::new(kind, message);
        let _ = self.logger.log(&entry);
    }
    fn wait_for_lifecycle(&mut self, _target: LifecycleTarget) -> anyhow::Result<WaitOutcome> {
        let mut ms = self.milestones.borrow_mut();
        if ms.len() > 1 {
            ms.remove(0);
        }
        Ok(WaitOutcome::Reached)
    }
    fn plan_dir(&self) -> &Path {
        &self.plan_dir
    }
    fn record_handoff(&mut self, transition: &str) -> anyhow::Result<()> {
        self.handoffs.borrow_mut().push(transition.to_string());
        Ok(())
    }
}

// ─── MEDIUM-1: prompt_source events land in `<watch.log>` ────────────────────

/// Default case: with no project-local override, every stage emits
/// a `prompt_source` event whose message ends in `→ default`.
/// The log is JSONL; a JSON parser confirms the `kind` field.
#[test]
fn prompt_source_default_event_lands_in_watch_log() {
    let tmp = tempfile::TempDir::new().unwrap();
    // Empty `<plan_dir>/watch/` — override_dir rung is skipped, the
    // plan_dir rung is skipped too, so the loader hits the compiled
    // default. The plan_dir argument is still required by the trait.
    let plan_dir = tmp.path().join("plan");
    std::fs::create_dir_all(plan_dir.join("watch")).unwrap();
    let log_path = tmp.path().join("watch.log");
    let logger = DriveLogger::open(&log_path).expect("open logger");

    let mut ops = LoggedScripted::new(
        vec![
            ms("1", "approved"),
            ms("1", "in-progress"),
            ms("1", "complete"),
            ms("1", "complete"),
        ],
        plan_dir,
        logger,
    );
    let outcome = drive_milestone(&mut ops, 10).expect("drive");
    assert_eq!(outcome, DriveOutcome::Complete);

    let log_text = std::fs::read_to_string(ops.milestones_log_path()).expect("read log");
    // At least one prompt_source event was emitted. Pin the kind so
    // a future rename breaks the test rather than silently landing
    // in the wrong field.
    let prompt_source_count = log_text
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|v| v["kind"] == "prompt_source")
        .count();
    let prompt_source_kinds: Vec<String> = log_text
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|v| v["kind"].as_str().map(|s| s.to_string()))
        .collect();
    assert!(
        prompt_source_count >= 1,
        "expected ≥ 1 prompt_source event in the watch log; got kinds={prompt_source_kinds:?}; full log:\n{log_text}"
    );
    // The default-case message format: `<stage_label> → default`.
    assert!(
        log_text.contains("→ default"),
        "default-case prompt_source message must end with '→ default'; full log:\n{log_text}"
    );
}

/// Override case: with `<plan_dir>/watch/execute.md` present,
/// the loader hits the project-local rung, and the emitted event
/// ends in `→ override (...)`.
#[test]
fn prompt_source_override_event_lands_in_watch_log() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plan_dir = tmp.path().join("plan");
    std::fs::create_dir_all(plan_dir.join("watch")).unwrap();
    std::fs::write(
        plan_dir.join("watch/execute.md"),
        "{header}OVERRIDE BODY for log probe — sentinel\n",
    )
    .unwrap();

    let log_path = tmp.path().join("watch.log");
    let logger = DriveLogger::open(&log_path).expect("open logger");

    let mut ops = LoggedScripted::new(
        vec![
            ms("1", "approved"),
            ms("1", "in-progress"),
            ms("1", "complete"),
            ms("1", "complete"),
        ],
        plan_dir,
        logger,
    );
    let outcome = drive_milestone(&mut ops, 10).expect("drive");
    assert_eq!(outcome, DriveOutcome::Complete);

    let log_text = std::fs::read_to_string(ops.milestones_log_path()).expect("read log");
    assert!(
        log_text.contains("→ override ("),
        "override-case prompt_source message must end with '→ override (...)'; full log:\n{log_text}"
    );
    // Path leak check: the override-path component must point to the
    // project-local override file, not an unrelated or absolute
    // path. The test pins the substring `<plan_dir>/watch/execute.md`
    // (the tempdir's expected layout) rather than a hardcoded
    // absolute path so the test survives `cargo test` running in
    // any cwd.
    let plan_dir_str = ops.plan_dir.join("watch/execute.md").display().to_string();
    assert!(
        log_text.contains(&plan_dir_str),
        "override event should reference the override file path; want substring {plan_dir_str}; full log:\n{log_text}"
    );
}

/// JSON shape: every `prompt_source` event is parseable JSON with
/// `kind == "prompt_source"` and a non-empty `message`. Defensive
/// against a future change that drops the message or changes the
/// JSON contract.
#[test]
fn prompt_source_events_have_parseable_jsonl_shape() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plan_dir = tmp.path().join("plan");
    std::fs::create_dir_all(plan_dir.join("watch")).unwrap();
    let log_path = tmp.path().join("watch.log");
    let logger = DriveLogger::open(&log_path).expect("open logger");

    let mut ops = LoggedScripted::new(
        vec![
            ms("1", "approved"),
            ms("1", "in-progress"),
            ms("1", "complete"),
            ms("1", "complete"),
        ],
        plan_dir,
        logger,
    );
    let _ = drive_milestone(&mut ops, 10).expect("drive");

    let log_text = std::fs::read_to_string(ops.milestones_log_path()).expect("read log");
    for line in log_text.lines() {
        let v: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("log line is not valid JSON: {e}; line={line}"));
        if v["kind"] == "prompt_source" {
            assert_eq!(
                v["kind"], "prompt_source",
                "prompt_source entry must carry kind=prompt_source"
            );
            assert!(
                v["message"].as_str().unwrap_or("").contains("→ "),
                "prompt_source message must contain the separator `→ `; got: {v:?}"
            );
            // `ts` is always present (WallClock at log time).
            assert!(
                v.get("ts").is_some(),
                "prompt_source events carry a timestamp: {v:?}"
            );
        }
    }
}

// ─── Unknown-stage override files are inert ─────────────────────────────────

/// Every drivable stage now ships a template file, so an override
/// only takes effect when its filename matches a stage label. A file
/// named after a stage that no longer exists (`re-review.md` was one
/// of the pre-cutover stage names) must not influence any rendered
/// prompt — the resolver keys on `PromptStage::label()`, so there is
/// no lookup that can reach it.
#[test]
fn override_file_for_a_non_stage_name_is_never_rendered() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plan_dir = tmp.path().join("plan");
    std::fs::create_dir_all(plan_dir.join("watch")).unwrap();

    std::fs::write(
        plan_dir.join("watch/re-review.md"),
        "{header}OPERATOR WROTE A BODY FOR A STAGE THAT DOES NOT EXIST (must NOT leak)\n",
    )
    .unwrap();

    let m = ms("1", "complete");
    let opts = mp::autopilot::drive::PromptRenderOptions::default();
    for stage in mp::autopilot::drive::all_stages() {
        let (text, source) =
            mp::autopilot::drive::build_prompt_with(stage, &m, &opts, None, Some(&plan_dir));
        assert_eq!(
            source,
            mp::autopilot::drive::TemplateSource::CompiledDefault,
            "stage {stage:?} must fall through to the compiled default; got {source:?}"
        );
        assert!(
            !text.contains("OPERATOR WROTE A BODY"),
            "stage {stage:?} prompt must not include the stray file's body; got:\n{text}"
        );
    }
}

// ─── BuildPromptRequest struct refactor (LOW-4) ─────────────────────────────

/// The current `build_prompt_with` takes 5 positional args. This
/// test drives the struct form when present and pins the
/// equivalence: calling via `BuildPromptRequest` must produce the
/// same `(text, source)` as the legacy 5-arg call for the same
/// milestone + same inputs.
#[test]
fn build_prompt_request_struct_matches_legacy_positional_args() {
    use mp::autopilot::drive::PromptRenderOptions;

    let tmp = tempfile::TempDir::new().unwrap();
    let plan_dir = tmp.path().join("plan");
    std::fs::create_dir_all(plan_dir.join("watch")).unwrap();

    let m = ms("M999", "approved");
    let opts = PromptRenderOptions::default();

    let (legacy_text, legacy_source) = mp::autopilot::drive::build_prompt_with(
        PromptStage::Execute,
        &m,
        &opts,
        None,
        Some(&plan_dir),
    );

    let req = BuildPromptRequest {
        stage: PromptStage::Execute,
        milestone: &m,
        options: &opts,
        override_dir: None,
        plan_dir: Some(plan_dir.as_path()),
    };
    let (struct_text, struct_source) =
        mp::autopilot::drive::build_prompt_with_request(&req).expect("build_prompt_with_request");

    assert_eq!(
        legacy_text, struct_text,
        "request struct must match legacy positional args"
    );
    assert_eq!(
        legacy_source, struct_source,
        "source attribution must match legacy positional args"
    );
}
