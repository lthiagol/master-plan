//! M176 S5 / AC-05: every quantitative perf AC must either wrap its claim in
//! `mp_measure!` or carry an explicit `manual:` prefix.
//!
//! Walks every milestone under master-plan/milestones/, reads each AC's
//! `verification` field, and fails if a threshold pattern
//! `(≥|>=).*?(s|%|min|sec)` appears without one of those gates.

mod common;

use crate::common::repo_root;
use regex::Regex;
use serde_json::Value;
use std::fs;
use std::path::Path;

#[test]
fn perf_ac_threshold_reality_check() {
    let milestones_dir = repo_root().join("master-plan/milestones");
    assert!(
        milestones_dir.is_dir(),
        "expected plan milestones at {}",
        milestones_dir.display()
    );

    // Numeric threshold: ≥ / >= + number + unit (s/sec/seconds/%/min/minutes).
    let threshold = Regex::new(r"(≥|>=)\s*\d+(\.\d+)?\s*(s|sec|seconds|%|min|minutes)\b")
        .expect("threshold regex");
    let mut offenders: Vec<String> = Vec::new();

    let mut entries: Vec<_> = fs::read_dir(&milestones_dir)
        .expect("read milestones dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json"))
        .collect();
    entries.sort();

    for path in entries {
        scan_milestone(&path, &threshold, &mut offenders);
    }

    assert!(
        offenders.is_empty(),
        "perf ACs without mp_measure! or manual: prefix ({}):\n  {}",
        offenders.len(),
        offenders.join("\n  ")
    );
}

fn scan_milestone(path: &Path, threshold: &Regex, offenders: &mut Vec<String>) {
    let raw = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return,
    };
    let v: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return,
    };
    let Some(acs) = v.get("acceptance_criteria").and_then(|a| a.as_array()) else {
        return;
    };
    let file = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    for ac in acs {
        let id = ac.get("id").and_then(|x| x.as_str()).unwrap_or("?");
        let verification = ac
            .get("verification")
            .and_then(|x| x.as_str())
            .unwrap_or("");
        if verification.is_empty() {
            continue;
        }
        if !threshold.is_match(verification) {
            continue;
        }
        let gated = verification.contains("mp_measure!")
            || verification.trim_start().starts_with("manual:");
        if !gated {
            offenders.push(format!("{file} {id}: {verification}"));
        }
    }
}
