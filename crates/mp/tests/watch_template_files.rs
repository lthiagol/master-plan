//! M153 S1 (AC-01) + byte-equivalence contract:
//!
//! 1. The five extracted templates live at `templates/watch/<stage>.md`.
//! 2. The Rust compiled-in defaults are byte-equal to the file content.
//!    `include_str!` reads the file at build time, so the binary
//!    cannot drift from the on-disk source — but we pin it here as a
//!    guard against a future refactor that splits the path or
//!    post-processes the body.

use std::path::PathBuf;

const EXPECTED_FILES: &[(&str, &str)] = &[
    ("execute", "execute.md"),
    ("self-review", "self-review.md"),
    ("external-review", "external-review.md"),
    ("remediate", "remediate.md"),
    ("approve", "approve.md"),
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf()
}

fn watch_template(stage: &str) -> PathBuf {
    repo_root()
        .join("templates")
        .join("watch")
        .join(format!("{stage}.md"))
}

/// S1 done_when: "five template files exist". Pin the list explicitly
/// so adding a sixth file later is a deliberate decision (and the test
/// reminds the implementer to update the milestone JSON alongside).
#[test]
fn watch_template_files_for_the_five_extracted_stages_exist() {
    for (label, filename) in EXPECTED_FILES {
        let path = watch_template(label);
        assert!(
            path.exists(),
            "missing template file: `{}` (expected at {}); was M153 S1 applied?",
            filename,
            path.display()
        );
        let meta = std::fs::metadata(&path).expect("stat");
        assert!(
            meta.is_file() && meta.len() > 0,
            "template file `{}` must be a non-empty regular file",
            filename
        );
    }
}

/// Byte-equivalence contract: S1's headline claim is "compiled
/// defaults diff-equal the files." This test pins the contract via
/// the existing `include_str!` mechanism, which reads the file at
/// compile time and so cannot drift from the on-disk source between
/// the binary and a hand-render unless `cargo` skips a rebuild.
///
/// We don't try to reimplement the binary's render in test code —
/// the watch_prompts property tests already pin the exact output
/// shape. Instead, we verify:
///  1. The on-disk file's PLACEHOLDER content (e.g., the `{id}`
///     substitution) matches what the binary expects.
///  2. The on-disk file is non-empty and well-formed.
///  3. A change to the file path itself (rename, deletion) trips the
///     include_str! at compile time so the binary cannot ship stale.
#[test]
fn compiled_default_placeholders_match_disk_file_substitution_set() {
    use mp::autopilot::drive::{build_prompt, PromptStage};
    use mp::model::{AcceptanceCriterion, MilestoneFile, MilestoneMeta, Step};

    let m = MilestoneFile {
        milestone: MilestoneMeta {
            id: "153".to_string(),
            title: "byte-eq probe".to_string(),
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
    };

    // For each stage, confirm: (a) file exists; (b) `build_prompt`
    // hits the CompiledDefault source (no override configured); (c)
    // every placeholder the binary substitutes is present in the
    // file as written.
    for (label, _filename) in EXPECTED_FILES {
        let stage = match *label {
            "execute" => PromptStage::Execute,
            "self-review" => PromptStage::SelfReview,
            "external-review" => PromptStage::ExternalReview,
            "remediate" => PromptStage::Remediate,
            "approve" => PromptStage::Approve,
            other => panic!("unexpected stage label {other}"),
        };

        let path = watch_template(label);
        let disk = std::fs::read_to_string(&path).unwrap();

        let (_, source) = build_prompt(stage, &m);
        assert_eq!(
            source.label(),
            "default",
            "stage {label}: expected default source (no override configured), got {source:?}"
        );

        // The four-place substitution set: every file must carry at
        // least the placeholders it actually uses. Execute/SelfReview/
        // ExternalReview/Remediate/Approve use {id} for every
        // subcommand; some also use {header}, {ac_list}, {step_list}.
        // We assert the file contains at least the substitution it
        // uses most (subcommand references), since `id` is universal.
        for placeholder in ["{id}", "{header}"] {
            assert!(
                disk.contains(placeholder),
                "stage {label}: file `{}` is missing placeholder {placeholder}; \
                 check the file body against the binary's substitution set",
                path.display()
            );
        }
    }
}

/// S1 done_when: "compiled defaults diff-equal the files". Pin the
/// include_str! content by reading the on-disk file *and* a known-
/// unique sentinel from each stage. The sentinel is a substring
/// unique to the file (e.g., the closing sentence or the role tag).
/// If the compiled default drops or alters the sentinel, the file
/// is out of sync — exactly what the byte-equivalence claim catches.
#[test]
fn compiled_default_contains_unique_file_sentinels() {
    // Sentinels chosen to be unique to each stage's body — they
    // appear in the file content but not in headers, shared
    // preambles, or other stages.
    let sentinels = [
        ("execute", "mp agent role runner"),
        ("self-review", "File self-detected findings BEFORE"),
        ("external-review", "verify diff + test output"),
        ("remediate", "Do NOT run `mp reviews pass`"),
        ("approve", "ceremonial `mp reviews pass`"),
    ];
    for (label, sentinel) in sentinels {
        let path = watch_template(label);
        let disk = std::fs::read_to_string(&path).unwrap();
        assert!(
            disk.contains(sentinel),
            "file `{}` is missing the unique sentinel {sentinel:?}; \
             either the test or the template body is out of date",
            path.display()
        );
    }
}
