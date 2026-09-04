//! M92 AC-07: `mp init` in a fresh dir creates JSON plan artifacts (.json
//! extensions) and `mp doctor` passes.

use crate::common::TestEnv;

#[test]
fn init_creates_json_plan_artifacts() {
    let env = TestEnv::blank();
    let out = env.run(&["init", "--profile", "full", "--format", "json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let plan = env.tmp.path().join("master-plan");
    // Core artifacts must have .json extensions (no .toml).
    for artifact in [
        "plan.json",
        "config.json",
        "brief.json",
        "ideas.json",
        "backlog.json",
    ] {
        assert!(
            plan.join(artifact).exists(),
            "init should scaffold {artifact}"
        );
    }
    // No TOML artifacts anywhere under the plan dir.
    for entry in walkdir::WalkDir::new(&plan) {
        let entry = entry.unwrap();
        if entry.file_type().is_file() {
            assert_ne!(
                entry.path().extension().and_then(|e| e.to_str()),
                Some("toml"),
                "init must not scaffold .toml artifacts: {}",
                entry.path().display()
            );
        }
    }
    // Tracks scaffolded as JSON too.
    assert!(plan.join("tracks/bugfix.json").exists());
    assert!(plan.join("tracks/tweak.json").exists());
    assert!(plan.join("archive/meta.json").exists());

    // doctor passes on the scaffolded plan.
    // CI runs ship without `herdr` on PATH, so doctor would
    // surface the missing-herdr gate and exit non-zero even
    // though the plan is healthy. Stub a herdr that satisfies
    // `which_herdr` + the `agent start --help` / `pane split
    // --help` shape probes so the test stays self-contained.
    let path = crate::common::fake_herdr::install_fake_herdr_for_doctor(&env);
    let doc = env.run_with_env(&[("PATH", &path)], &["doctor", "--format", "json"]);
    assert!(
        doc.status.success(),
        "{}",
        String::from_utf8_lossy(&doc.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&doc.stdout).unwrap();
    assert_eq!(v["ok"].as_bool(), Some(true));
}

/// Each init profile scaffolds a valid JSON config with the right profile.
#[test]
fn init_each_profile_scaffolds_json_config() {
    for profile in ["full", "hybrid", "session"] {
        let env = TestEnv::blank();
        let out = env.run(&["init", "--profile", profile, "--format", "json"]);
        assert!(
            out.status.success(),
            "profile {profile}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let cfg_path = env.tmp.path().join("master-plan/config.json");
        assert!(cfg_path.exists(), "config.json missing for {profile}");
        let cfg: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&cfg_path).unwrap()).unwrap();
        assert_eq!(cfg["workflow"]["profile"].as_str().unwrap(), profile);
    }
}
