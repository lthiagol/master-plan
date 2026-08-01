use std::path::PathBuf;

#[path = "../src/scenario.rs"]
mod scenario;

mod common;

use common::repo_root;

#[test]
fn run_implemented_scenarios() {
    let root = repo_root();
    let mp = PathBuf::from(common::mp_bin());
    let results = scenario::run_all_implemented(&root, &mp).expect("run scenarios");
    for result in &results {
        if result.message == "skipped (phase=planned)" {
            continue;
        }
        assert!(result.passed, "{}: {}", result.id, result.message);
    }
}
