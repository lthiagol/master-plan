//! M153 S2: project-local override resolution and source attribution.
//!
//! The watch loop reads `<plan_dir>/watch/<stage>.md` (or an
//! explicit `override_dir`) before falling back to the compiled
//! default. `build_prompt_with` returns `(text, TemplateSource)`
//! so the caller can log which surface served the body.
//!
//! These tests pin:
//! 1. With no override file → `TemplateSource::CompiledDefault`.
//! 2. With `<plan_dir>/watch/<stage>.md` → `TemplateSource::ProjectOverride`.
//! 3. With a caller-supplied `override_dir` → `TemplateSource::ProjectOverride`.
//! 4. `override_dir` wins over `plan_dir` (lookup order is explicit).
//! 5. The override's content is what reaches the rendered prompt.
//! 6. `load_override` (the standalone helper) errors when no file
//!    exists at either rung.

use mp::autopilot::drive::{
    build_prompt_with, load_override, PromptRenderOptions, PromptStage, TemplateSource,
};
use mp::model::{AcceptanceCriterion, MilestoneFile, MilestoneMeta, Step};

fn fixture_milestone() -> MilestoneFile {
    MilestoneFile {
        milestone: MilestoneMeta {
            id: "153".to_string(),
            title: "override probe".to_string(),
            ..Default::default()
        },
        acceptance_criteria: vec![AcceptanceCriterion {
            id: "AC-01".to_string(),
            description: "control ac".to_string(),
            verification: "manual".to_string(),
            status: "pending".to_string(),
            evidence: String::new(),
        }],
        steps: vec![Step {
            id: "S1".to_string(),
            action: "control step".to_string(),
            status: "pending".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    }
}

// ─── Project-local override resolution ────────────────────────────────────

#[test]
fn project_local_override_resolves_when_file_is_present() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plan_dir = tmp.path();
    let watch_dir = plan_dir.join("watch");
    std::fs::create_dir_all(&watch_dir).unwrap();

    // Override for execute stage. Distinct sentinel so a regression
    // that drops the override (and falls back to the default) shows
    // up as a missing substring rather than a substring-match race.
    let override_path = watch_dir.join("execute.md");
    std::fs::write(
        &override_path,
        "{header}OVERRIDDEN BODY — sentinel from project watch/execute.md\n",
    )
    .unwrap();

    let m = fixture_milestone();
    let opts = PromptRenderOptions::default();
    let (text, source) = build_prompt_with(PromptStage::Execute, &m, &opts, None, Some(plan_dir));

    assert_eq!(
        source,
        TemplateSource::ProjectOverride(override_path.clone()),
        "expected project-local override to win; got {source:?}"
    );
    assert!(
        text.contains("OVERRIDDEN BODY"),
        "rendered prompt must include the override body: {text}"
    );
    // The header (computed in Rust) is still applied on top of the
    // override body. Pins the contract that header substitution
    // happens AFTER the override is loaded.
    assert!(
        text.starts_with("# mp watch — execute M153:"),
        "header must come from the Rust context regardless of override: {text}"
    );
}

#[test]
fn no_override_falls_through_to_compiled_default() {
    let tmp = tempfile::TempDir::new().unwrap();
    // Don't create watch/ subdir — the project-local rung is absent.
    let plan_dir = tmp.path();

    let m = fixture_milestone();
    let opts = PromptRenderOptions::default();
    let (_text, source) = build_prompt_with(PromptStage::Execute, &m, &opts, None, Some(plan_dir));

    assert_eq!(
        source,
        TemplateSource::CompiledDefault,
        "with no override file and a plan_dir, the loader must report CompiledDefault; got {source:?}"
    );
}

#[test]
fn override_dir_path_takes_priority_over_plan_dir() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plan_dir = tmp.path().join("plan");
    let override_dir = tmp.path().join("cli-override");
    std::fs::create_dir_all(plan_dir.join("watch")).unwrap();
    std::fs::create_dir_all(&override_dir).unwrap();

    // Distinct sentinels per rung. The override_dir rung wins, so
    // the rendered prompt contains ONLY "CLIOVER" and not
    // "PROJECTOVER".
    std::fs::write(
        plan_dir.join("watch/execute.md"),
        "{header}PROJECTOVER (must lose)\n",
    )
    .unwrap();
    std::fs::write(
        override_dir.join("execute.md"),
        "{header}CLIOVER (must win)\n",
    )
    .unwrap();

    let m = fixture_milestone();
    let opts = PromptRenderOptions::default();
    let (text, source) = build_prompt_with(
        PromptStage::Execute,
        &m,
        &opts,
        Some(&override_dir),
        Some(&plan_dir),
    );

    assert!(
        matches!(source, TemplateSource::ProjectOverride(ref p) if p == &override_dir.join("execute.md")),
        "override_dir must win over plan_dir; got {source:?}"
    );
    assert!(text.contains("CLIOVER (must win)"));
    assert!(
        !text.contains("PROJECTOVER"),
        "the project-local rung must NOT have been used; rendered: {text}"
    );
}

#[test]
fn override_dir_is_used_even_when_plan_dir_would_also_resolve() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plan_dir = tmp.path().join("plan");
    let override_dir = tmp.path().join("cli");
    std::fs::create_dir_all(plan_dir.join("watch")).unwrap();
    std::fs::create_dir_all(&override_dir).unwrap();

    std::fs::write(
        plan_dir.join("watch/external-review.md"),
        "{header}proj-only-fallback\n",
    )
    .unwrap();
    std::fs::write(
        override_dir.join("external-review.md"),
        "{header}cli-wins\n",
    )
    .unwrap();

    let m = fixture_milestone();
    let opts = PromptRenderOptions::default();
    let (_text, source) = build_prompt_with(
        PromptStage::ExternalReview,
        &m,
        &opts,
        Some(&override_dir),
        Some(&plan_dir),
    );

    assert!(
        matches!(source, TemplateSource::ProjectOverride(ref p) if p == &override_dir.join("external-review.md")),
        "external-review override_dir must be used; got {source:?}"
    );
}

#[test]
fn override_dir_resolves_when_plan_dir_is_absent() {
    let tmp = tempfile::TempDir::new().unwrap();
    let override_dir = tmp.path().join("cli");
    std::fs::create_dir_all(&override_dir).unwrap();
    std::fs::write(
        override_dir.join("approve.md"),
        "{header}approve override (no plan_dir)\n",
    )
    .unwrap();

    let m = fixture_milestone();
    let opts = PromptRenderOptions::default();
    let (_text, source) =
        build_prompt_with(PromptStage::Approve, &m, &opts, Some(&override_dir), None);

    assert!(
        matches!(source, TemplateSource::ProjectOverride(_)),
        "override_dir alone must resolve to ProjectOverride; got {source:?}"
    );
}

// ─── Stage-by-stage override coverage ─────────────────────────────────────

/// For every externalized stage, the loader honors an override file.
/// One test, many checkpoints — cheaper than five near-identical
/// tests and the assertion is the same: the override renders.
#[test]
fn overrides_resolve_for_every_externalized_stage() {
    let tmp = tempfile::TempDir::new().unwrap();
    let override_dir = tmp.path().join("cli");
    std::fs::create_dir_all(&override_dir).unwrap();

    let stages = [
        (PromptStage::Execute, "execute", "EXEC-SENTINEL"),
        (
            PromptStage::SelfReview,
            "self-review",
            "SELFREVIEW-SENTINEL",
        ),
        (
            PromptStage::ExternalReview,
            "external-review",
            "EXTREVIEW-SENTINEL",
        ),
        (PromptStage::Remediate, "remediate", "REMEDIATE-SENTINEL"),
        (PromptStage::Approve, "approve", "APPROVE-SENTINEL"),
    ];
    for (_stage, label, sentinel) in stages {
        std::fs::write(
            override_dir.join(format!("{label}.md")),
            format!("{{header}}{sentinel}\n"),
        )
        .unwrap();
    }

    let m = fixture_milestone();
    let opts = PromptRenderOptions::default();
    for (stage, _label, sentinel) in stages {
        let (text, source) = build_prompt_with(stage, &m, &opts, Some(&override_dir), None);
        assert!(
            matches!(source, TemplateSource::ProjectOverride(_)),
            "stage {stage:?} should resolve to override; got {source:?}"
        );
        assert!(
            text.contains(sentinel),
            "stage {stage:?}: rendered prompt must contain {sentinel}; rendered: {text}"
        );
    }
}

// ─── load_override helper ──────────────────────────────────────────────────

#[test]
fn load_override_helper_returns_project_override_when_present() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plan_dir = tmp.path().join("plan");
    let override_dir = tmp.path().join("cli");
    std::fs::create_dir_all(plan_dir.join("watch")).unwrap();
    std::fs::create_dir_all(&override_dir).unwrap();

    std::fs::write(
        plan_dir.join("watch/remediate.md"),
        "{header}from-plan-dir\n",
    )
    .unwrap();
    std::fs::write(
        override_dir.join("remediate.md"),
        "{header}from-override-dir\n",
    )
    .unwrap();

    let (text, source) =
        load_override(PromptStage::Remediate, Some(&plan_dir), Some(&override_dir))
            .expect("override file should resolve");
    assert!(
        matches!(source, TemplateSource::ProjectOverride(_)),
        "load_override must report ProjectOverride; got {source:?}"
    );
    assert!(text.contains("from-override-dir"));
}

#[test]
fn load_override_helper_errors_when_no_file_is_present() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plan_dir = tmp.path().join("plan");
    std::fs::create_dir_all(&plan_dir).unwrap();
    // Don't create watch/ — neither rung has a file.

    let err = load_override(PromptStage::Execute, Some(&plan_dir), None)
        .expect_err("missing override must error");
    assert_eq!(
        err.kind(),
        std::io::ErrorKind::NotFound,
        "load_override must surface a NotFound error so callers can distinguish missing-file from other I/O failures"
    );
}

// ─── Source-attribution contract ───────────────────────────────────────────

#[test]
fn template_source_label_distinguishes_override_from_default() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plan_dir = tmp.path().join("plan");
    std::fs::create_dir_all(plan_dir.join("watch")).unwrap();
    std::fs::write(plan_dir.join("watch/remediate.md"), "{header}proj-o\n").unwrap();

    let m = fixture_milestone();
    let opts = PromptRenderOptions::default();

    let (_text, source_override) =
        build_prompt_with(PromptStage::Remediate, &m, &opts, None, Some(&plan_dir));
    assert_eq!(source_override.label(), "override", "override label");
    assert!(source_override.is_override());

    let (_text, source_default) = build_prompt_with(PromptStage::Remediate, &m, &opts, None, None);
    assert_eq!(source_default.label(), "default");
    assert!(!source_default.is_override());

    // ReReview stays hardcoded (M153 S1 ships 5 files).
    let (_text, source_hardcoded) = build_prompt_with(PromptStage::ReReview, &m, &opts, None, None);
    assert_eq!(source_hardcoded.label(), "re-review");
}

// ─── Pickup parity: override does not lose the header ──────────────────────

#[test]
fn override_body_combines_with_rust_computed_header() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plan_dir = tmp.path().join("plan");
    std::fs::create_dir_all(plan_dir.join("watch")).unwrap();
    std::fs::write(
        plan_dir.join("watch/execute.md"),
        "{header}<<OVERRIDE BODY>>\n**runner instructions** go here\n",
    )
    .unwrap();

    let m = fixture_milestone();
    let opts = PromptRenderOptions::default();
    let (text, _) = build_prompt_with(PromptStage::Execute, &m, &opts, None, Some(&plan_dir));

    // Header substitutions (`# mp watch — execute M153:`, the
    // SAFETY preamble) come from Rust, not the file. The override
    // body is appended after the header.
    assert!(text.contains("# mp watch — execute M153:"));
    assert!(text.contains("SAFETY:"));
    assert!(text.contains("<<OVERRIDE BODY>>"));
}

// ─── Override safety guard (M153 S2 HIGH-4) ─────────────────────────────────

/// An override file that drops the `{header}` placeholder strips the
/// SAFETY preamble and trust-boundary tags. The loader must refuse
/// such an override and fall back to the compiled default rather
/// than silently rendering a header-less prompt.
#[test]
fn headerless_override_is_refused_and_falls_back_to_default() {
    let tmp = tempfile::TempDir::new().unwrap();
    let override_dir = tmp.path().join("cli");
    std::fs::create_dir_all(&override_dir).unwrap();
    // Deliberately omit the `{header}` placeholder. Loader should
    // refuse this and fall back to CompiledDefault.
    std::fs::write(
        override_dir.join("execute.md"),
        "PROBE OVERRIDE — header placeholder absent\n",
    )
    .unwrap();

    let m = fixture_milestone();
    let opts = PromptRenderOptions::default();
    let (text, source) =
        build_prompt_with(PromptStage::Execute, &m, &opts, Some(&override_dir), None);

    assert_eq!(
        source,
        TemplateSource::CompiledDefault,
        "headerless override must fall through to CompiledDefault; got {source:?}"
    );
    assert!(
        !text.contains("PROBE OVERRIDE"),
        "refused override body must NOT leak into the rendered prompt: {text}"
    );
    assert!(
        text.contains("SAFETY:"),
        "refused override must still produce the SAFETY preamble from the compiled default"
    );
    assert!(
        text.contains("<title>override probe</title>"),
        "refused override must still produce the title trust boundary from the compiled default"
    );
}

/// Empty override files have the same safety profile as missing
/// ones — the loader ignores the file and falls back.
#[test]
fn empty_override_falls_back_to_default() {
    let tmp = tempfile::TempDir::new().unwrap();
    let override_dir = tmp.path().join("cli");
    std::fs::create_dir_all(&override_dir).unwrap();
    std::fs::write(override_dir.join("execute.md"), "").unwrap();

    let m = fixture_milestone();
    let opts = PromptRenderOptions::default();
    let (_text, source) =
        build_prompt_with(PromptStage::Execute, &m, &opts, Some(&override_dir), None);

    assert_eq!(
        source,
        TemplateSource::CompiledDefault,
        "empty override must report CompiledDefault; got {source:?}"
    );
}

/// Override that DOES include `{header}` (the safety guard's
/// positive case) is honored verbatim.
#[test]
fn headered_override_is_honored() {
    let tmp = tempfile::TempDir::new().unwrap();
    let override_dir = tmp.path().join("cli");
    std::fs::create_dir_all(&override_dir).unwrap();
    std::fs::write(
        override_dir.join("execute.md"),
        "{header}<<SAFETY OVERRIDE — header is present>>\n",
    )
    .unwrap();

    let m = fixture_milestone();
    let opts = PromptRenderOptions::default();
    let (text, source) =
        build_prompt_with(PromptStage::Execute, &m, &opts, Some(&override_dir), None);

    assert!(
        matches!(source, TemplateSource::ProjectOverride(_)),
        "headered override must resolve; got {source:?}"
    );
    assert!(text.contains("<<SAFETY OVERRIDE — header is present>>"));
}

// ─── M153 ext-review F-09: load_override safety parity ─────────────────────

/// F-09: the exported `load_override` helper must apply the same
/// safety validation as the canonical renderer. A headerless
/// override is treated as if no override file existed (returns
/// `Err(NotFound)` so the caller falls through to the compiled
/// default). The helper used to bypass this guard entirely.
#[test]
fn load_override_helper_rejects_headerless_overrides() {
    let tmp = tempfile::TempDir::new().unwrap();
    let override_dir = tmp.path().join("cli");
    std::fs::create_dir_all(&override_dir).unwrap();
    // Deliberately omit `{header}`. load_override must refuse this.
    std::fs::write(
        override_dir.join("execute.md"),
        "PROBE OVERRIDE — header placeholder absent\n",
    )
    .unwrap();

    let err = load_override(PromptStage::Execute, None, Some(&override_dir))
        .expect_err("headerless override must be refused");
    assert_eq!(
        err.kind(),
        std::io::ErrorKind::NotFound,
        "load_override must collapse a refused override into NotFound so callers fall through; got {:?}",
        err
    );
    assert!(
        err.to_string().contains("no usable override"),
        "error message should explain the refusal: {err}"
    );
}

/// F-09: an empty override file is rejected with the same NotFound
/// fallback as a missing file. The safety invariant (HIGH-4) is
/// unchanged: no override reaches the caller without `{header}`.
#[test]
fn load_override_helper_rejects_empty_overrides() {
    let tmp = tempfile::TempDir::new().unwrap();
    let override_dir = tmp.path().join("cli");
    std::fs::create_dir_all(&override_dir).unwrap();
    std::fs::write(override_dir.join("execute.md"), "").unwrap();

    let err = load_override(PromptStage::Execute, None, Some(&override_dir))
        .expect_err("empty override must be refused");
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}

/// F-09: a headered override round-trips through `load_override`.
/// Pins the positive case so a future tightening doesn't regress.
#[test]
fn load_override_helper_returns_text_for_headered_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    let override_dir = tmp.path().join("cli");
    std::fs::create_dir_all(&override_dir).unwrap();
    std::fs::write(
        override_dir.join("execute.md"),
        "{header}CUSTOM EXECUTE BODY\n",
    )
    .unwrap();

    let (body, source) =
        load_override(PromptStage::Execute, None, Some(&override_dir)).expect("load");
    assert!(body.contains("CUSTOM EXECUTE BODY"));
    assert!(
        matches!(source, TemplateSource::ProjectOverride(_)),
        "got {source:?}"
    );
}

// ─── M153 ext-review F-11 / F-12: shared reader surface ─────────────────────

use mp::autopilot::drive::{
    build_prompt_full, OverrideRefusalKind, OverrideRung, MAX_OVERRIDE_BYTES,
};

/// F-11 / F-12: a headerless project-local override surfaces a
/// structured diagnostic. The dry-run and live state machine both
/// consume this through `build_prompt_full`. The diagnostic carries
/// the rung, the refusal kind, and the path.
#[test]
fn headerless_plan_dir_override_emits_structured_diagnostic() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plan_dir = tmp.path();
    let watch_dir = plan_dir.join("watch");
    std::fs::create_dir_all(&watch_dir).unwrap();
    std::fs::write(watch_dir.join("execute.md"), "PROBE — header missing\n").unwrap();

    let m = fixture_milestone();
    let opts = PromptRenderOptions::default();
    let req = mp::autopilot::drive::BuildPromptRequest {
        stage: PromptStage::Execute,
        milestone: &m,
        options: &opts,
        override_dir: None,
        plan_dir: Some(plan_dir),
    };
    let rendered = build_prompt_full(&req, MAX_OVERRIDE_BYTES);
    assert_eq!(rendered.source, TemplateSource::CompiledDefault);
    assert_eq!(
        rendered.override_diagnostics.len(),
        1,
        "exactly one diagnostic"
    );
    let d = &rendered.override_diagnostics[0];
    assert_eq!(d.kind, OverrideRefusalKind::HeaderMissing);
    assert_eq!(d.rung, OverrideRung::PlanDir);
    assert!(d.message.contains("{header}"));
}

/// F-11: an empty plan_dir override surfaces a structured diagnostic
/// with kind=Empty and the file path that was refused.
#[test]
fn empty_plan_dir_override_emits_empty_diagnostic() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plan_dir = tmp.path();
    let watch_dir = plan_dir.join("watch");
    std::fs::create_dir_all(&watch_dir).unwrap();
    std::fs::write(watch_dir.join("execute.md"), "").unwrap();

    let m = fixture_milestone();
    let opts = PromptRenderOptions::default();
    let req = mp::autopilot::drive::BuildPromptRequest {
        stage: PromptStage::Execute,
        milestone: &m,
        options: &opts,
        override_dir: None,
        plan_dir: Some(plan_dir),
    };
    let rendered = build_prompt_full(&req, MAX_OVERRIDE_BYTES);
    assert_eq!(rendered.override_diagnostics.len(), 1);
    let d = &rendered.override_diagnostics[0];
    assert_eq!(d.kind, OverrideRefusalKind::Empty);
}

/// F-12: a directory at the override rung is rejected with
/// kind=NotRegular. Rejection happens BEFORE the read so a large
/// directory tree does not allocate unbounded memory.
#[test]
fn directory_override_is_rejected_as_not_regular() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plan_dir = tmp.path();
    let watch_dir = plan_dir.join("watch");
    std::fs::create_dir_all(&watch_dir).unwrap();
    // `execute.md` is created as a directory instead of a file.
    std::fs::create_dir_all(watch_dir.join("execute.md")).unwrap();

    let m = fixture_milestone();
    let opts = PromptRenderOptions::default();
    let req = mp::autopilot::drive::BuildPromptRequest {
        stage: PromptStage::Execute,
        milestone: &m,
        options: &opts,
        override_dir: None,
        plan_dir: Some(plan_dir),
    };
    let rendered = build_prompt_full(&req, MAX_OVERRIDE_BYTES);
    assert_eq!(rendered.override_diagnostics.len(), 1);
    let d = &rendered.override_diagnostics[0];
    assert_eq!(d.kind, OverrideRefusalKind::NotRegular);
}

/// F-12: an oversized override is rejected with kind=TooLarge before
/// allocation. We exercise this with a tiny test cap so the test
/// doesn't have to write a 1 MiB file.
#[test]
fn oversized_override_is_rejected_as_too_large() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plan_dir = tmp.path();
    let watch_dir = plan_dir.join("watch");
    std::fs::create_dir_all(&watch_dir).unwrap();
    // Pad with comment lines so the file exceeds the test cap of
    // 256 bytes but is otherwise well-formed (contains {header}).
    let mut body = String::from("{header}");
    body.push_str(&"x".repeat(300));
    std::fs::write(watch_dir.join("execute.md"), body).unwrap();

    let m = fixture_milestone();
    let opts = PromptRenderOptions::default();
    let req = mp::autopilot::drive::BuildPromptRequest {
        stage: PromptStage::Execute,
        milestone: &m,
        options: &opts,
        override_dir: None,
        plan_dir: Some(plan_dir),
    };
    let rendered = build_prompt_full(&req, 256);
    assert_eq!(rendered.override_diagnostics.len(), 1);
    let d = &rendered.override_diagnostics[0];
    assert_eq!(d.kind, OverrideRefusalKind::TooLarge);
    assert!(d.message.contains("256"));
}

/// F-12: invalid UTF-8 is rejected with kind=InvalidUtf8.
#[test]
fn invalid_utf8_override_is_rejected_with_diagnostic() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plan_dir = tmp.path();
    let watch_dir = plan_dir.join("watch");
    std::fs::create_dir_all(&watch_dir).unwrap();
    // 0xFF is not valid UTF-8 anywhere.
    let bytes: Vec<u8> = b"{header}\xff\xff\xff".to_vec();
    std::fs::write(watch_dir.join("execute.md"), bytes).unwrap();

    let m = fixture_milestone();
    let opts = PromptRenderOptions::default();
    let req = mp::autopilot::drive::BuildPromptRequest {
        stage: PromptStage::Execute,
        milestone: &m,
        options: &opts,
        override_dir: None,
        plan_dir: Some(plan_dir),
    };
    let rendered = build_prompt_full(&req, MAX_OVERRIDE_BYTES);
    assert_eq!(rendered.override_diagnostics.len(), 1);
    let d = &rendered.override_diagnostics[0];
    assert_eq!(d.kind, OverrideRefusalKind::InvalidUtf8);
}

/// F-11: missing-file (NotFound) is the ONLY silent case. The
/// diagnostics list is empty when no rung has a file.
#[test]
fn no_file_at_either_rung_produces_no_diagnostic() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plan_dir = tmp.path();
    // Don't create watch/ — neither rung has a file.

    let m = fixture_milestone();
    let opts = PromptRenderOptions::default();
    let req = mp::autopilot::drive::BuildPromptRequest {
        stage: PromptStage::Execute,
        milestone: &m,
        options: &opts,
        override_dir: None,
        plan_dir: Some(plan_dir),
    };
    let rendered = build_prompt_full(&req, MAX_OVERRIDE_BYTES);
    assert!(
        rendered.override_diagnostics.is_empty(),
        "missing files must NOT produce diagnostics; got {:?}",
        rendered.override_diagnostics
    );
    assert_eq!(rendered.source, TemplateSource::CompiledDefault);
}

/// F-11: lookup precedence — a refused higher rung falls through
/// to a valid lower rung while retaining the diagnostic.
#[test]
fn refused_override_dir_falls_through_to_valid_plan_dir_with_warning() {
    let tmp = tempfile::TempDir::new().unwrap();
    let override_dir = tmp.path().join("cli");
    let plan_dir = tmp.path().join("plan");
    std::fs::create_dir_all(&override_dir).unwrap();
    std::fs::create_dir_all(plan_dir.join("watch")).unwrap();

    // Override-dir rung: headerless (refused).
    std::fs::write(override_dir.join("execute.md"), "BAD — no header\n").unwrap();
    // Plan-dir rung: valid.
    std::fs::write(
        plan_dir.join("watch/execute.md"),
        "{header}PLAN-DIR BODY (must win via fallthrough)\n",
    )
    .unwrap();

    let m = fixture_milestone();
    let opts = PromptRenderOptions::default();
    let req = mp::autopilot::drive::BuildPromptRequest {
        stage: PromptStage::Execute,
        milestone: &m,
        options: &opts,
        override_dir: Some(&override_dir),
        plan_dir: Some(&plan_dir),
    };
    let rendered = build_prompt_full(&req, MAX_OVERRIDE_BYTES);

    // The valid plan_dir rung wins, but the refusal is observable.
    assert!(
        matches!(rendered.source, TemplateSource::ProjectOverride(ref p) if p.ends_with("watch/execute.md")),
        "got source: {:?}",
        rendered.source
    );
    assert!(
        rendered.text.contains("PLAN-DIR BODY"),
        "plan-dir body must have been used: {}",
        rendered.text
    );
    assert_eq!(rendered.override_diagnostics.len(), 1);
    assert_eq!(
        rendered.override_diagnostics[0].rung,
        OverrideRung::OverrideDir
    );
    assert_eq!(
        rendered.override_diagnostics[0].kind,
        OverrideRefusalKind::HeaderMissing
    );
}

/// F-12 (Unix-only): a symlink is rejected as not_regular without
/// chasing the link. cfg-gated so Windows / non-Unix CI doesn't
/// break.
#[cfg(unix)]
#[test]
fn symlink_override_is_rejected_as_not_regular() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plan_dir = tmp.path();
    let watch_dir = plan_dir.join("watch");
    std::fs::create_dir_all(&watch_dir).unwrap();

    // Build a real file then symlink it into the override path.
    let real = plan_dir.join("real.md");
    std::fs::write(&real, "{header}real body\n").unwrap();
    std::os::unix::fs::symlink(&real, watch_dir.join("execute.md")).unwrap();

    let m = fixture_milestone();
    let opts = PromptRenderOptions::default();
    let req = mp::autopilot::drive::BuildPromptRequest {
        stage: PromptStage::Execute,
        milestone: &m,
        options: &opts,
        override_dir: None,
        plan_dir: Some(plan_dir),
    };
    let rendered = build_prompt_full(&req, MAX_OVERRIDE_BYTES);
    assert_eq!(rendered.override_diagnostics.len(), 1);
    assert_eq!(
        rendered.override_diagnostics[0].kind,
        OverrideRefusalKind::NotRegular
    );
}

/// F-12 (Unix-only): a FIFO would block the reader indefinitely.
/// The regular-file policy rejects it before the read starts.
#[cfg(unix)]
#[test]
fn fifo_override_is_rejected_as_not_regular() {
    use std::os::unix::fs::FileTypeExt;
    let tmp = tempfile::TempDir::new().unwrap();
    let plan_dir = tmp.path();
    let watch_dir = plan_dir.join("watch");
    std::fs::create_dir_all(&watch_dir).unwrap();
    // mkfifo via libc — `OpenOptionsExt` opens an existing file, it
    // does not create a FIFO.
    let path = watch_dir.join("execute.md");
    let path_c = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
    let rc = unsafe { libc::mkfifo(path_c.as_ptr(), 0o600) };
    assert_eq!(rc, 0, "mkfifo failed: {}", std::io::Error::last_os_error());
    // Sanity: the path is a FIFO, not a regular file.
    let meta = std::fs::symlink_metadata(&path).unwrap();
    assert!(
        meta.file_type().is_fifo(),
        "test setup: file must be a FIFO; got {:?}",
        meta.file_type()
    );

    let m = fixture_milestone();
    let opts = PromptRenderOptions::default();
    let req = mp::autopilot::drive::BuildPromptRequest {
        stage: PromptStage::Execute,
        milestone: &m,
        options: &opts,
        override_dir: None,
        plan_dir: Some(plan_dir),
    };
    let rendered = build_prompt_full(&req, MAX_OVERRIDE_BYTES);
    assert_eq!(rendered.override_diagnostics.len(), 1);
    assert_eq!(
        rendered.override_diagnostics[0].kind,
        OverrideRefusalKind::NotRegular
    );
}
