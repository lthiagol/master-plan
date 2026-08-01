use anyhow::Result;
use serde_json::Value;

use crate::mp_runner::MpRunner;

/// First line of a backlog item description (mp uses `description`, not `title`).
pub fn backlog_summary(item: &Value) -> String {
    item["description"]
        .as_str()
        .or_else(|| item["title"].as_str())
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

pub fn status(runner: &MpRunner) -> Result<Value> {
    runner.run("status", &[])
}

pub fn list_milestones(runner: &MpRunner, sort: Option<&str>) -> Result<Value> {
    let mut args: Vec<&str> = vec!["milestones"];
    if let Some(s) = sort {
        args.push("--sort");
        args.push(s);
    }
    runner.run("list", &args)
}

pub fn show_milestone(runner: &MpRunner, id: &str) -> Result<Value> {
    runner.run("show", &["milestone", id])
}

pub fn next_step(runner: &MpRunner) -> Result<Value> {
    runner.run("next", &[])
}

pub fn path(runner: &MpRunner) -> Result<Value> {
    runner.run("path", &[])
}

/// M102/M103: read all 4 path lanes from `mp path --all`.
pub fn path_lanes(runner: &MpRunner) -> Result<Value> {
    runner.run("path", &["--all"])
}

/// M103: read a single lane by name.
pub fn path_lane(runner: &MpRunner, lane: &str) -> Result<Value> {
    runner.run("path", &["--lane", lane])
}

pub fn digest(runner: &MpRunner) -> Result<Value> {
    runner.run("digest", &[])
}

#[derive(Debug, Clone, Default)]
pub struct DigestOpts {
    pub since_handoff: bool,
    pub since: Option<String>,
    pub days: Option<u32>,
}

pub fn digest_with_opts(runner: &MpRunner, opts: DigestOpts) -> Result<Value> {
    let mut args: Vec<String> = Vec::new();
    if opts.since_handoff {
        args.push("--since-handoff".to_string());
    } else if let Some(since) = opts.since {
        args.push("--since".to_string());
        args.push(since);
    } else if let Some(days) = opts.days {
        args.push("--days".to_string());
        args.push(days.to_string());
    }
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    runner.run("digest", &arg_refs)
}

pub fn graph(runner: &MpRunner) -> Result<Value> {
    runner.run("graph", &[])
}

pub fn list_annotations(runner: &MpRunner, open_only: bool, target: Option<&str>) -> Result<Value> {
    let mut args: Vec<&str> = vec!["list"];
    if open_only {
        args.push("--open");
    }
    if let Some(t) = target {
        args.push("--target");
        args.push(t);
    }
    runner.run("annotation", &args)
}

pub fn list_decisions(runner: &MpRunner) -> Result<Value> {
    runner.run("list", &["decisions"])
}

pub fn inbox(runner: &MpRunner) -> Result<Value> {
    runner.run("inbox", &[])
}

/// M180 / M181: read the consolidated Overview snapshot from `mp
/// overview`. The payload is the new project-wide read model (see
/// `crates/raul/src/overview_snapshot.rs`); Raul consumes it as-is
/// rather than fanning out status / inbox / path / reviews / watch
/// subprocesses for one dashboard load.
pub fn overview(runner: &MpRunner) -> Result<Value> {
    runner.run("overview", &[])
}

/// Run validate and return raw stdout bytes (validate exits non-zero on errors).
pub fn validate(runner: &MpRunner) -> Result<Vec<u8>> {
    runner.run_raw_allow_failure("validate", &[])
}

pub fn milestone_impact(runner: &MpRunner, id: &str) -> Result<Value> {
    runner.run("milestone", &["impact", id])
}

pub fn list_tracks(runner: &MpRunner) -> Result<Value> {
    runner.run("list", &["tracks"])
}

pub fn show_track(runner: &MpRunner, kind: &str) -> Result<Value> {
    runner.run("track", &["show", kind])
}

pub fn list_backlog(runner: &MpRunner) -> Result<Value> {
    runner.run("list", &["backlog"])
}

/// Condensed spec-review projection (M80): `mp spec review <id>`.
pub fn spec_review(runner: &MpRunner, id: &str) -> Result<Value> {
    runner.run("spec", &["review", id])
}

/// Spec diff since last approval (M80): `mp spec diff <id>`.
pub fn spec_diff(runner: &MpRunner, id: &str) -> Result<Value> {
    runner.run("spec", &["diff", id])
}

/// M103 ER-4: typed view of `mp path --all` output. Used by the contract
/// pin (frozen fixture + deserialize test) so a lane-shape drift in M102
/// breaks the build before it breaks the human rendering.
///
/// The struct mirrors the shape `crates/mp/src/path_engine.rs::Lane`
/// emits. Field naming follows the JSON exactly so the deserialize stays
/// trivial; if M102 renames a field, the deserialize test below fails.
#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
pub struct PathAction {
    pub rank: u32,
    #[serde(rename = "type")]
    pub item_type: String,
    pub milestone: PathActionMilestone,
    pub step: Option<serde_json::Value>,
    pub work_package: Option<serde_json::Value>,
    pub reason: String,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
pub struct PathActionMilestone {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub lifecycle: String,
    #[serde(default)]
    pub priority: String,
    #[serde(default)]
    pub review_phase: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub display: String,
    #[serde(default)]
    pub needs_regrooming: bool,
    #[serde(default)]
    pub open_external_findings: u32,
    #[serde(default)]
    pub open_self_findings: u32,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LaneItemType {
    Step,
    Milestone,
    BacklogItem,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
pub struct Lane {
    pub name: String,
    pub item_type: LaneItemType,
    pub item_count: usize,
    pub head: Option<PathAction>,
    pub items: Vec<PathAction>,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
pub struct PathLanes {
    pub strategy: String,
    pub lanes: Vec<Lane>,
    #[serde(default)]
    pub summary: std::collections::BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
pub struct PathLaneSingle {
    pub name: String,
    pub item_type: LaneItemType,
    pub item_count: usize,
    pub head: Option<PathAction>,
    pub items: Vec<PathAction>,
}

#[cfg(test)]
mod path_lanes_contract_tests {
    //! M103 ER-4: pin the `mp path --all` / `--lane <name>` wire shape
    //! against a frozen fixture in `crates/raul/tests/contract/`. A
    //! rename or removal on the mp side surfaces here as a deserialize
    //! failure rather than as a silent misrender at the user end.

    use super::*;
    use std::path::PathBuf;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    fn load(name: &str) -> serde_json::Value {
        let path = workspace_root()
            .join("crates/raul/tests/contract")
            .join(name);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read fixture {path:?}: {e}"));
        serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("parse fixture {path:?}: {e}"))
    }

    #[test]
    fn path_lanes_deserializes_against_frozen_contract() {
        let raw = load("path_lanes_schema.json");
        let parsed: PathLanes = serde_json::from_value(raw)
            .expect("path_lanes_schema.json must deserialize into PathLanes");
        // M157: 6 lanes — blocked, execution, awaiting-approval, review,
        // grooming, backlog.
        assert_eq!(parsed.lanes.len(), 6, "expected 6 lanes (M157)");
        assert_eq!(parsed.lanes[0].name, "blocked");
        assert_eq!(parsed.lanes[0].item_type, LaneItemType::Milestone);
        assert_eq!(parsed.lanes[0].items.len(), 0);
        assert_eq!(parsed.lanes[1].name, "execution");
        assert_eq!(parsed.lanes[1].item_type, LaneItemType::Milestone);
        assert_eq!(parsed.lanes[1].items.len(), 2);
        assert_eq!(parsed.lanes[2].name, "awaiting-approval");
        assert_eq!(parsed.lanes[5].name, "backlog");
        assert_eq!(parsed.lanes[5].item_type, LaneItemType::BacklogItem);
        assert_eq!(parsed.lanes[5].items[0].milestone.id, "BF-01");
    }

    #[test]
    fn path_lane_single_deserializes_against_frozen_contract() {
        let raw = load("path_lane_single_schema.json");
        let parsed: PathLaneSingle = serde_json::from_value(raw)
            .expect("path_lane_single_schema.json must deserialize into PathLaneSingle");
        assert_eq!(parsed.name, "execution");
        assert_eq!(parsed.item_count, 1);
        assert_eq!(parsed.items[0].milestone.id, "110");
    }

    #[test]
    fn path_lane_empty_deserializes_against_frozen_contract() {
        let raw = load("path_lane_empty_schema.json");
        let parsed: PathLaneSingle = serde_json::from_value(raw)
            .expect("path_lane_empty_schema.json must deserialize into PathLaneSingle");
        assert_eq!(parsed.name, "backlog");
        assert_eq!(parsed.item_count, 0);
        assert!(parsed.items.is_empty());
        assert!(parsed.head.is_none());
    }
}
