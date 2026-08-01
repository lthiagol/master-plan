use crate::common::TestEnv;

#[test]
fn sweep_has_correct_buckets() {
    let env = TestEnv::new();

    let out = env.run(&["reviews", "sweep", "--format", "json"]);
    assert!(
        out.status.success(),
        "sweep failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v.get("total").is_some());
    let buckets = v["buckets"].as_array().unwrap();
    assert_eq!(buckets.len(), 3);

    let kinds: Vec<&str> = buckets
        .iter()
        .map(|b| b["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"force-bypassed"));
    assert!(kinds.contains(&"manual-only"));
    assert!(kinds.contains(&"runnable"));
}

#[test]
fn sweep_counts_sum_to_total() {
    let env = TestEnv::new();

    let out = env.run(&["reviews", "sweep", "--format", "json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let total = v["total"].as_u64().unwrap();
    let buckets = v["buckets"].as_array().unwrap();
    let sum: u64 = buckets.iter().map(|b| b["count"].as_u64().unwrap()).sum();
    assert_eq!(sum, total);
}

#[test]
fn sweep_works_with_fields_projection() {
    let env = TestEnv::new();

    let out = env.run(&[
        "reviews",
        "sweep",
        "--fields",
        "total,buckets[].kind,buckets[].count",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v.get("total").is_some());
    let buckets = v["buckets"].as_array().unwrap();
    for b in buckets {
        assert!(b.get("kind").is_some());
        assert!(b.get("count").is_some());
        assert!(b.get("milestone_ids").is_none());
    }
}
