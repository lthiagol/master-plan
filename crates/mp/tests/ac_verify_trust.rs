use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use mp::ac_verify::run_one_in;
use mp_model::AcceptanceCriterion;

fn mp_bin() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_mp"))
}

fn run(command: &str, cwd: &Path) -> mp::ac_verify::AcResult {
    let ac = AcceptanceCriterion {
        id: "AC-TRUST".into(),
        description: "trust fixture".into(),
        verification: command.into(),
        status: "pending".into(),
        evidence: String::new(),
    };
    run_one_in(
        &ac,
        Some(cwd),
        &Arc::new(AtomicBool::new(false)),
        &Arc::new(Mutex::new(Vec::new())),
        None,
    )
}

/// L15: do **not** canonicalize the temp root before exercising the trust
/// gate. On macOS, tempfile lives under `/var/folders` where `/var` is a
/// symlink to `/private/var`; pre-canonicalizing masks the CLI/path bug
/// where ancestor platform symlinks were over-rejected.
#[test]
fn ac_verify_trust_gate_and_execution_modes() {
    let temp = tempfile::TempDir::new().expect("temp");
    let root = temp.path().join("repo");
    std::fs::create_dir_all(&root).expect("repo");
    // Intentionally keep the non-canonical lexical path (e.g. macOS
    // /var/folders/... where /var → /private/var). Do not canonicalize.
    let trust_store = temp.path().join("config/trusted-repositories.json");
    std::env::set_var("MP_VERIFY_TRUST_STORE", &trust_store);
    std::env::remove_var("MP_VERIFY_TRUST_REPOSITORY");
    std::env::remove_var("MP_VERIFY_ALLOW_SHELL");

    let touch_marker = temp.path().join("touch-marker");
    let substitution_marker = temp.path().join("substitution-marker");
    let redirect_marker = temp.path().join("redirect-marker");
    let credential = temp.path().join("credential");
    let credential_copy = temp.path().join("credential-copy");
    std::fs::write(&credential, "secret").expect("credential fixture");

    for command in [
        format!("touch {}", touch_marker.display()),
        format!("echo $(touch {})", substitution_marker.display()),
        format!("printf pwned > {}", redirect_marker.display()),
        format!(
            "cat {} > {}",
            credential.display(),
            credential_copy.display()
        ),
    ] {
        let result = run(&command, &root);
        assert!(!result.passed, "untrusted command must fail: {command}");
        assert!(
            result.note.contains("not trusted"),
            "failure must identify the trust gate (not platform symlink prefix): {}",
            result.note
        );
        assert!(
            !result.note.contains("symlinked path"),
            "platform ancestor symlinks must not block trust: {}",
            result.note
        );
    }
    for marker in [
        &touch_marker,
        &substitution_marker,
        &redirect_marker,
        &credential_copy,
    ] {
        assert!(!marker.exists(), "untrusted command created {marker:?}");
    }

    std::env::set_var("MP_VERIFY_TRUST_REPOSITORY", "1");
    let granted = run("echo argv-ok", &root);
    assert!(granted.passed, "explicit trust should persist: {granted:?}");
    assert!(trust_store.is_file());
    std::env::remove_var("MP_VERIFY_TRUST_REPOSITORY");

    let persisted = run("echo persisted-ok", &root);
    assert!(persisted.passed, "persisted repo trust should be reused");

    let rejected_operator = run(
        &format!("printf blocked > {}", redirect_marker.display()),
        &root,
    );
    assert!(!rejected_operator.passed);
    assert!(rejected_operator.note.contains("argv-only"));
    assert!(!redirect_marker.exists());

    std::env::set_var("MP_VERIFY_ALLOW_SHELL", "1");
    let shell = run(
        &format!("printf shell-ok > {}", redirect_marker.display()),
        &root,
    );
    assert!(shell.passed, "explicit trusted shell should run: {shell:?}");
    assert_eq!(
        std::fs::read_to_string(&redirect_marker).expect("shell marker"),
        "shell-ok"
    );
    std::env::remove_var("MP_VERIFY_ALLOW_SHELL");

    #[cfg(unix)]
    {
        let alias = temp.path().join("repo-alias");
        std::os::unix::fs::symlink(&root, &alias).expect("repo symlink");
        let result = run("echo must-not-run", &alias);
        assert!(!result.passed);
        assert!(
            result.note.contains("symlinked path"),
            "symlink alias must not inherit trust: {}",
            result.note
        );
        assert!(
            result.note.contains("repo-alias"),
            "must reject the repo-root alias path, got: {}",
            result.note
        );
        // Not the platform prefix alone (…/symlinked path /var).
        assert!(
            !result.note.ends_with("symlinked path /var")
                && !result.note.ends_with("symlinked path /tmp"),
            "must not reject only a system prefix: {}",
            result.note
        );
    }
}

/// CLI-path regression (L15): invoke `mp milestone verify` with a
/// non-canonical `--project-root` under the platform temp prefix. Must reach
/// the normal trust fail-closed message, not "symlinked path /var".
#[test]
fn ac_verify_trust_cli_noncanonical_project_root() {
    let temp = tempfile::TempDir::new().expect("temp");
    // Keep the lexical tempfile path; do not canonicalize before verify.
    let project: PathBuf = temp.path().to_path_buf();
    let trust_store = temp.path().join("trusted-repositories.json");

    let init = Command::new(mp_bin())
        .current_dir(&project)
        .args(["init", "--profile", "full", "--format", "json"])
        .output()
        .expect("mp init");
    assert!(
        init.status.success(),
        "mp init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let create = Command::new(mp_bin())
        .current_dir(&project)
        .args([
            "milestone",
            "create",
            "--format",
            "json",
            "--json",
            r#"{"title":"trust-cli","intent":{"outcome":"o"},"problem":{"description":"p"},"scope":{"in_scope":["s"],"out_of_scope":["x","y"]},"acceptance_criteria":[{"description":"run","verification":"true"}]}"#,
        ])
        .output()
        .expect("mp milestone create");
    assert!(
        create.status.success(),
        "mp milestone create failed: {}",
        String::from_utf8_lossy(&create.stderr)
    );
    let created: serde_json::Value = serde_json::from_slice(&create.stdout).expect("create json");
    let mid = created["milestone"]["id"].as_str().expect("milestone id");

    let verify = Command::new(mp_bin())
        .args([
            "milestone",
            "verify",
            mid,
            "--project-root",
            project.to_str().expect("utf8 project root"),
            "--format",
            "json",
        ])
        .env("MP_VERIFY_TRUST_STORE", &trust_store)
        .env_remove("MP_VERIFY_TRUST_REPOSITORY")
        .env_remove("MP_VERIFY_ALLOW_SHELL")
        .output()
        .expect("mp milestone verify");
    let stdout = String::from_utf8_lossy(&verify.stdout);
    let stderr = String::from_utf8_lossy(&verify.stderr);
    let body = if stdout.trim().starts_with('{') {
        stdout.to_string()
    } else {
        format!("{stdout}{stderr}")
    };
    assert!(
        body.contains("not trusted"),
        "non-canonical project_root must reach trust fail-closed, got: {body}"
    );
    assert!(
        !body.contains("symlinked path"),
        "must not reject macOS system prefix symlinks on CLI path: {body}"
    );
}
