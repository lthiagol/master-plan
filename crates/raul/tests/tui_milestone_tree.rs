//! M185: former tree renderer removed — pin depends_on indent depths
//! on the Table path instead.

use raul::tui::app::{App, Lane, MilestoneSummary};
use raul::tui::render::lane_lists::depends_on_depths;
use std::collections::BTreeMap;

fn ms(id: &str, deps: &[&str]) -> MilestoneSummary {
    MilestoneSummary {
        id: id.into(),
        title: format!("t{id}"),
        lifecycle: "approved".into(),
        lifecycle_at: None,
        depends_on: deps.iter().map(|s| (*s).to_string()).collect(),
        priority: "normal".into(),
        updated: String::new(),
        created: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: BTreeMap::new(),
    }
}

#[test]
fn tui_milestone_tree_production_depends_on_flows_through_app() {
    let mut app = App::new();
    app.load_milestones(vec![ms("01", &[]), ms("02", &["01"]), ms("03", &["02"])]);
    app.select_lane(Lane::Milestones);
    let visible = app.visible_milestones();
    let depths = depends_on_depths(&visible);
    // M01 root=0, M02 child=1, M03 grandchild=2
    let by_id: std::collections::HashMap<&str, usize> = visible
        .iter()
        .zip(depths.iter())
        .map(|(m, d)| (m.id.as_str(), *d))
        .collect();
    assert_eq!(by_id.get("01"), Some(&0));
    assert_eq!(by_id.get("02"), Some(&1));
    assert_eq!(by_id.get("03"), Some(&2));
}
