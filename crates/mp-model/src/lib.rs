mod config;
mod datetime;
mod milestone;
mod normalize;
mod plan;
mod track;

pub use config::*;
pub use datetime::*;
pub use milestone::*;
pub use plan::*;
pub use track::*;

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_steps_merges_legacy_work_package_steps() {
        let mut m = MilestoneFile {
            work_packages: vec![WorkPackage {
                id: "WP1".to_string(),
                name: "OAuth".to_string(),
                goal: String::new(),
                rollback: String::new(),
                steps: vec![Step {
                    id: "S1".to_string(),
                    action: "configure".to_string(),
                    tests: String::new(),
                    done_when: String::new(),
                    status: "pending".to_string(),
                    ..Default::default()
                }],
            }],
            ..Default::default()
        };
        m.normalize_steps_from_disk().unwrap();
        assert_eq!(m.steps.len(), 1);
        assert_eq!(m.steps[0].work_package, "WP1");
        assert!(m.work_packages[0].steps.is_empty());
    }

    #[test]
    fn top_level_vs_nested_step_id_conflict_reports_both_locations() {
        let mut m = MilestoneFile {
            steps: vec![Step {
                id: "S1".to_string(),
                action: "canonical".to_string(),
                tests: String::new(),
                done_when: String::new(),
                status: "pending".to_string(),
                ..Default::default()
            }],
            work_packages: vec![WorkPackage {
                id: "WP1".to_string(),
                name: String::new(),
                goal: String::new(),
                rollback: String::new(),
                steps: vec![Step {
                    id: "S1".to_string(),
                    action: "legacy".to_string(),
                    tests: String::new(),
                    done_when: String::new(),
                    status: "pending".to_string(),
                    ..Default::default()
                }],
            }],
            ..Default::default()
        };
        let error = m.normalize_steps_from_disk().unwrap_err();
        assert!(
            error.contains("steps[0]"),
            "must report top-level location: {error}"
        );
        assert!(
            error.contains("work_packages[0](WP1).steps[0]"),
            "must report nested location: {error}"
        );
        // Precedence: top-level step is retained; nested was not merged.
        assert_eq!(m.steps.len(), 1);
        assert_eq!(m.steps[0].action, "canonical");
    }

    #[test]
    fn duplicate_nested_step_ids_report_both_locations() {
        let step = |action: &str| Step {
            id: "S1".to_string(),
            action: action.to_string(),
            status: "pending".to_string(),
            ..Default::default()
        };
        let mut m = MilestoneFile {
            work_packages: vec![
                WorkPackage {
                    id: "WP1".to_string(),
                    steps: vec![step("first")],
                    ..Default::default()
                },
                WorkPackage {
                    id: "WP2".to_string(),
                    steps: vec![step("second")],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let error = m.normalize_steps_from_disk().unwrap_err();
        assert!(error.contains("work_packages[0](WP1).steps[0]"));
        assert!(error.contains("work_packages[1](WP2).steps[0]"));
    }

    #[test]
    fn milestone_file_roundtrip() {
        let m = MilestoneFile {
            milestone: MilestoneMeta {
                id: "42".into(),
                title: "Test".into(),
                slug: "test".into(),
                lifecycle: "approved".into(),
                lifecycle_at: Some("2026-07-04T00:00:00Z".into()),
                spec_status: "ready".into(),
                execution_status: "planned".into(),
                blocked: false,
                needs_regrooming: false,
                cancelled: false,
                deferred: false,
                deferred_reason: String::new(),
                depends_on: vec!["40".into()],
                effort: "M".into(),
                risk: "low".into(),
                change_kind: "greenfield".into(),
                priority: "high".into(),
                created: "2026-01-01".into(),
                updated: "2026-01-02".into(),
                blocked_at: String::new(),
                block_reason: String::new(),
                blocked_by: String::new(),
                target_version: String::new(),
                executed_by: String::new(),
                remediation_pre_state: None,
            },
            intent: Intent {
                outcome: "It works".into(),
            },
            problem: Problem {
                description: "Needed".into(),
            },
            scope: Scope {
                in_scope: vec!["X".into()],
                out_of_scope: vec!["Y".into(), "Z".into()],
            },
            acceptance_criteria: vec![AcceptanceCriterion {
                id: "AC-01".into(),
                description: "Works".into(),
                verification: "manual".into(),
                status: "pending".into(),
                evidence: String::new(),
            }],
            steps: vec![Step {
                id: "S1".into(),
                action: "do it".into(),
                tests: "make test".into(),
                done_when: "done".into(),
                status: "pending".into(),
                work_package: "WP1".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let json = serde_json::to_string_pretty(&m).unwrap();
        let m2: MilestoneFile = serde_json::from_str(&json).unwrap();
        assert_eq!(m2.milestone.id, "42");
        assert_eq!(m2.milestone.title, "Test");
        assert_eq!(m2.steps.len(), 1);
        assert_eq!(m2.scope.out_of_scope.len(), 2);
        assert_eq!(m2.acceptance_criteria[0].id, "AC-01");
    }

    #[test]
    fn plan_file_roundtrip() {
        let p = PlanFile {
            project: ProjectMeta {
                name: "test".into(),
                description: "desc".into(),
                stack: vec!["rust".into()],
                platforms: vec!["linux".into()],
                created: "2026-01-01".into(),
                target_version: "1.0".into(),
                planning_status: "active".into(),
                planning_phase: "milestones".into(),
            },
            ..Default::default()
        };
        let json = serde_json::to_string_pretty(&p).unwrap();
        let p2: PlanFile = serde_json::from_str(&json).unwrap();
        assert_eq!(p2.project.name, "test");
    }

    #[test]
    fn track_file_roundtrip() {
        let t = TrackFile {
            track: TrackMeta {
                kind: "bugfix".into(),
                title: "Fix crash".into(),
                perpetual: false,
                scope: "app".into(),
                created: "2026-01-01".into(),
            },
            items: vec![],
        };
        let json = serde_json::to_string_pretty(&t).unwrap();
        let t2: TrackFile = serde_json::from_str(&json).unwrap();
        assert_eq!(t2.track.kind, "bugfix");
    }

    #[test]
    fn idea_entry_roundtrip() {
        let i = IdeaEntry {
            id: "ID-01".into(),
            title: "Dark mode".into(),
            body: "Add dark mode".into(),
            status: "open".into(),
            tags: vec!["ux".into()],
            source: "brainstorm".into(),
            created: "2026-01-01".into(),
            promoted_to: String::new(),
        };
        let json = serde_json::to_string_pretty(&i).unwrap();
        let i2: IdeaEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(i2.title, "Dark mode");
    }

    #[test]
    fn delta_is_set() {
        let d = MilestoneDelta {
            domain: "auth".into(),
            ..Default::default()
        };
        assert!(d.is_set());
        let d2 = MilestoneDelta::default();
        assert!(!d2.is_set());
    }

    #[test]
    fn execution_config_defaults() {
        let cfg = ExecutionConfig::default();
        assert_eq!(cfg.strategy, "resume_then_ready");
        assert_eq!(cfg.interleave, "milestone");
        assert_eq!(cfg.mode, "planning");
    }
}
