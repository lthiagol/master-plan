//! M173 S3: `mp docgen` smoke test. The generator is exercised
//! against a per-test output dir so concurrent runs don't race on
//! generated command fragments (default under a temp out-dir in tests).
//! Tests cover: (a) every top-level subcommand emits a `.md` file,
//! (b) each file is non-empty, (c) leaf commands carry an Options
//! table, (d) parent commands carry a Subcommands table, (e) the
//! filter (`--group`) emits only the requested file.

mod common;

use common::TestEnv;

fn run_docgen(env: &TestEnv, args: &[&str]) -> std::process::Output {
    let out_dir = env.tmp.path().join("docgen-out");
    let root = common::repo_root();
    let plan_dir = env.tmp.path().join("master-plan");
    std::fs::create_dir_all(&plan_dir).unwrap();

    let mut cmd = std::process::Command::new(common::mp_bin());
    cmd.current_dir(&root)
        .env("MP_HOME", &root)
        .arg("--plan-dir")
        .arg(&plan_dir)
        .arg("docgen")
        .arg("--out")
        .arg(&out_dir)
        .args(args);
    cmd.output().expect("spawn mp")
}

#[test]
fn docgen_emits_markdown_for_every_command_group() {
    let env = TestEnv::blank();
    let out = run_docgen(&env, &[]);
    assert!(
        out.status.success(),
        "docgen must succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert!(v["files_written"].as_u64().unwrap() >= 5);

    let out_dir = env.tmp.path().join("docgen-out");
    let groups: Vec<String> = v["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g.as_str().unwrap().to_string())
        .collect();
    assert!(!groups.is_empty(), "no groups reported");
    for g in &groups {
        let path = out_dir.join(format!("{g}.md"));
        assert!(
            path.is_file(),
            "expected generated file for group {g} at {}",
            path.display()
        );
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            !content.is_empty(),
            "generated file for {g} must not be empty"
        );
        assert!(
            content.contains(&format!("# `mp {g}`")),
            "generated file for {g} must have a top-level heading; got:\n{content}"
        );
    }
}

#[test]
fn docgen_leaf_command_has_options_table() {
    let env = TestEnv::blank();
    let out = run_docgen(&env, &["--group", "search"]);
    assert!(out.status.success(), "docgen --group=search must succeed");
    let dest = env.tmp.path().join("docgen-out/search.md");
    let content = std::fs::read_to_string(&dest).unwrap();
    assert!(
        content.contains("**Options:**"),
        "leaf command `search` must have an Options table; got:\n{content}"
    );
    assert!(
        content.contains("--type") || content.contains("--limit") || content.contains("--include"),
        "search Options table must enumerate at least one real flag"
    );
}

#[test]
fn docgen_parent_command_has_subcommands_table() {
    let env = TestEnv::blank();
    let out = run_docgen(&env, &["--group", "milestone"]);
    assert!(
        out.status.success(),
        "docgen --group=milestone must succeed"
    );
    let dest = env.tmp.path().join("docgen-out/milestone.md");
    let content = std::fs::read_to_string(&dest).unwrap();
    assert!(
        content.contains("**Subcommands:**"),
        "parent command `milestone` must have a Subcommands table; got:\n{content}"
    );
    assert!(
        content.contains("`create`") && content.contains("`approve`"),
        "milestone subcommands table must enumerate `create` and `approve`"
    );
}

#[test]
fn docgen_group_filter_emits_only_one_file() {
    let env = TestEnv::blank();
    let out = run_docgen(&env, &["--group", "install"]);
    assert!(out.status.success(), "docgen --group=install must succeed");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let groups = v["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 1, "filter must emit exactly one group");
    assert_eq!(groups[0].as_str().unwrap(), "install");
    assert_eq!(v["files_written"].as_u64().unwrap(), 1);

    let out_dir = env.tmp.path().join("docgen-out");
    let entries: Vec<_> = std::fs::read_dir(&out_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(entries, vec!["install.md".to_string()]);
}

/// Live command reference is hand-authored at `docs/mp/commands.md`
/// (replaces the retired generated MP-COMMANDS / docs-old tree).
#[test]
fn live_commands_md_covers_core_surfaces() {
    let root = common::repo_root();
    let md = std::fs::read_to_string(root.join("docs/mp/commands.md"))
        .expect("docs/mp/commands.md must exist");
    for needle in [
        "mp init",
        "mp install",
        "mp doctor",
        "mp milestone",
        "mp reviews",
        "mp validate",
    ] {
        assert!(
            md.contains(needle),
            "docs/mp/commands.md missing coverage for `{needle}`"
        );
    }
}
