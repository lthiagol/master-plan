use std::fs;
use std::io::Write;

use mp::json_input::{
    read_json_arg_in, read_json_payload_in, read_to_string_bounded, MAX_JSON_INPUT_BYTES,
};
use mp::{store, PlanContext};
use tempfile::TempDir;

#[test]
fn contained_relative_and_absolute_files_are_accepted() {
    let root = TempDir::new().unwrap();
    let input = root.path().join("input.json");
    fs::write(&input, "{}").unwrap();

    assert_eq!(
        read_json_arg_in(&format!("@{}", input.display()), Some(root.path())).unwrap(),
        "{}"
    );
    assert_eq!(
        read_json_payload_in(Some(&input), None, Some(root.path())).unwrap(),
        "{}"
    );
}

#[test]
fn streaming_reader_accepts_exact_limit_and_stops_after_one_extra_byte() {
    let mut exact = std::io::Cursor::new(vec![b' '; 1024]);
    assert_eq!(
        read_to_string_bounded(&mut exact, 1024, "exact reader")
            .unwrap()
            .len(),
        1024
    );
    assert_eq!(exact.position(), 1024);

    let mut growing = std::io::Cursor::new(vec![b' '; 4096]);
    let error = read_to_string_bounded(&mut growing, 1024, "growing reader")
        .unwrap_err()
        .to_string();
    assert!(error.contains("exceeds 1024 bytes"), "{error}");
    assert_eq!(
        growing.position(),
        1025,
        "reader must reject incrementally without consuming the full stream"
    );

    let exact_inline = " ".repeat(MAX_JSON_INPUT_BYTES as usize);
    assert!(read_json_arg_in(&exact_inline, None).is_ok());
    let oversized_inline = format!("{exact_inline} ");
    assert!(read_json_arg_in(&oversized_inline, None).is_err());
}

#[test]
fn every_durable_store_loader_rejects_oversized_json_while_reading() {
    let temp = TempDir::new().unwrap();
    let plan_dir = temp.path().join("master-plan");
    fs::create_dir_all(plan_dir.join("tracks")).unwrap();
    fs::create_dir_all(plan_dir.join("archive")).unwrap();
    let source = temp.path().join("oversized.json");
    let file = fs::File::create(&source).unwrap();
    file.set_len(store::MAX_PLAN_FILE_BYTES + 1).unwrap();

    let paths = [
        plan_dir.join("brief.json"),
        plan_dir.join("tracks/bugfix.json"),
        plan_dir.join("archive/meta.json"),
        plan_dir.join("backlog.json"),
        plan_dir.join("ideas.json"),
        plan_dir.join("annotations.json"),
        plan_dir.join("decisions.json"),
        temp.path().join("challenge.json"),
        temp.path().join("session.json"),
        plan_dir.join("reviews.json"),
        plan_dir.join("config.json"),
    ];
    for path in &paths {
        fs::hard_link(&source, path).unwrap();
    }
    let ctx = PlanContext {
        project_root: temp.path().to_path_buf(),
        plan_dir: plan_dir.clone(),
    };
    let errors = [
        store::load_brief(&ctx).unwrap_err(),
        store::load_track(&ctx, "bugfix").unwrap_err(),
        store::load_archive_meta(&ctx).unwrap_err(),
        store::load_backlog(&ctx).unwrap_err(),
        store::load_ideas(&ctx).unwrap_err(),
        store::load_annotations(&ctx).unwrap_err(),
        store::load_decisions(&ctx).unwrap_err(),
        store::load_challenge(&paths[7]).unwrap_err(),
        store::load_session_from_path(&paths[8]).unwrap_err(),
        mp::reviews::load_reviews_for_validate(&ctx)
            .map(|_| ())
            .unwrap_err(),
        store::try_load_config(&ctx).unwrap_err(),
    ];
    for error in errors {
        let message = format!("{error:#}");
        assert!(
            message.contains("exceeds 67108864 bytes"),
            "loader did not preserve bounded-reader error: {message}"
        );
        assert!(
            !message.contains("not found"),
            "size-limit failure must not be labeled not-found: {message}"
        );
    }

    // Session milestone.json discovery must not load an oversized file as a hit.
    let session_dir = plan_dir.join("sessions").join("s1");
    fs::create_dir_all(&session_dir).unwrap();
    fs::hard_link(&source, session_dir.join("milestone.json")).unwrap();
    assert!(
        mp::paths::find_milestone_in_ctx(&ctx, "188").is_none(),
        "oversized session milestone.json must not match via unbounded read"
    );
}

#[test]
fn oversized_challenge_preserves_size_error_not_not_found() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("challenge.json");
    let file = fs::File::create(&path).unwrap();
    file.set_len(store::MAX_PLAN_FILE_BYTES + 1).unwrap();
    let message = format!("{:#}", store::load_challenge(&path).unwrap_err());
    assert!(message.contains("exceeds 67108864 bytes"), "{message}");
    assert!(!message.contains("not found"), "{message}");
}

#[test]
fn missing_challenge_still_reports_not_found() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("missing-challenge.json");
    let message = format!("{:#}", store::load_challenge(&path).unwrap_err());
    assert!(message.contains("not found"), "{message}");
}

#[test]
fn durable_store_loader_accepts_an_exact_limit_json_document() {
    let temp = TempDir::new().unwrap();
    let plan_dir = temp.path().join("master-plan");
    fs::create_dir_all(&plan_dir).unwrap();
    let path = plan_dir.join("backlog.json");
    let mut file = std::io::BufWriter::new(fs::File::create(&path).unwrap());
    file.write_all(br#"{"items":[]}"#).unwrap();
    let padding = store::MAX_PLAN_FILE_BYTES as usize - br#"{"items":[]}"#.len();
    let spaces = vec![b' '; 1024 * 1024];
    let mut remaining = padding;
    while remaining > 0 {
        let count = remaining.min(spaces.len());
        file.write_all(&spaces[..count]).unwrap();
        remaining -= count;
    }
    file.flush().unwrap();
    drop(file);

    let ctx = PlanContext {
        project_root: temp.path().to_path_buf(),
        plan_dir,
    };
    assert!(store::load_backlog(&ctx).is_ok());
    assert_eq!(
        fs::metadata(path).unwrap().len(),
        store::MAX_PLAN_FILE_BYTES
    );
}

#[cfg(unix)]
#[test]
fn symlinks_are_allowed_only_when_the_complete_target_is_contained() {
    use std::os::unix::fs::symlink;

    let root = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let inside_target = root.path().join("inside.json");
    let outside_target = outside.path().join("outside.json");
    fs::write(&inside_target, r#"{"inside":true}"#).unwrap();
    fs::write(&outside_target, r#"{"outside":true}"#).unwrap();

    let inside_link = root.path().join("inside-link.json");
    symlink(&inside_target, &inside_link).unwrap();
    assert!(read_json_payload_in(Some(&inside_link), None, Some(root.path())).is_ok());

    let outside_link = root.path().join("outside-link.json");
    symlink(&outside_target, &outside_link).unwrap();
    for result in [
        read_json_arg_in(&format!("@{}", outside_link.display()), Some(root.path())),
        read_json_payload_in(Some(&outside_link), None, Some(root.path())),
    ] {
        let error = result.unwrap_err().to_string();
        assert!(error.contains("escapes project root"), "{error}");
    }

    let outside_dir_link = root.path().join("outside-dir");
    symlink(outside.path(), &outside_dir_link).unwrap();
    let error = read_json_payload_in(
        Some(&outside_dir_link.join("outside.json")),
        None,
        Some(root.path()),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("escapes project root"), "{error}");

    let broken = root.path().join("broken.json");
    symlink(root.path().join("missing.json"), &broken).unwrap();
    let error = read_json_payload_in(Some(&broken), None, Some(root.path()))
        .unwrap_err()
        .to_string();
    assert!(error.contains("canonicalize input path"), "{error}");
}
