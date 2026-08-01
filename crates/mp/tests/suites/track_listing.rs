use crate::common::TestEnv;

#[test]
fn track_list_default_is_summary_only() {
    let env = TestEnv::new();
    let out = env.run(&["track", "list", "--format", "json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let tracks = v["tracks"].as_array().unwrap();
    assert!(!tracks.is_empty());
    for track in tracks {
        assert!(track.get("kind").is_some());
        assert!(track.get("total").is_some());
        assert!(
            track.get("items").is_none(),
            "default should not include items"
        );
    }
}

#[test]
fn track_list_items_includes_items() {
    let env = TestEnv::new();
    let out = env.run(&["track", "list", "--items", "--format", "json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let tracks = v["tracks"].as_array().unwrap();
    for track in tracks {
        let items = track["items"].as_array().unwrap();
        for item in items {
            assert!(item.get("id").is_some());
            assert!(item.get("title").is_some());
            assert!(item.get("status").is_some());
            assert!(
                item.get("problem").is_none(),
                "items should not include full detail"
            );
        }
    }
}

#[test]
fn list_tracks_items_works() {
    let env = TestEnv::new();
    let out = env.run(&["list", "tracks", "--items", "--format", "json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let tracks = v["tracks"].as_array().unwrap();
    for track in tracks {
        assert!(track.get("items").is_some());
    }
}

#[test]
fn track_list_items_with_fields_projection() {
    let env = TestEnv::new();
    let out = env.run(&[
        "track",
        "list",
        "--items",
        "--fields",
        "tracks[].kind,tracks[].items[].id",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let tracks = v["tracks"].as_array().unwrap();
    for track in tracks {
        assert!(track.get("kind").is_some());
        assert!(track.get("items").is_some());
        assert!(track.get("title").is_none());
    }
}
