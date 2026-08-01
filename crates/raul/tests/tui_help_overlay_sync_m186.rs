//! M186 AC-09: help overlay + Settings keys include search + cycle_sort.

use raul::tui::keybinds::Keybinds;
use raul::tui::modes::settings::SETTINGS_KEYS;

#[test]
fn help_entries_list_search_and_cycle_sort() {
    let entries = Keybinds::default().help_entries();
    let labels: Vec<_> = entries.iter().map(|e| e.label).collect();
    assert!(
        labels.iter().any(|l| l.contains("Search")),
        "help must mention search; got {labels:?}"
    );
    assert!(
        labels.iter().any(|l| l.contains("Cycle sort")),
        "help must mention cycle sort; got {labels:?}"
    );
}

#[test]
fn settings_keys_include_search_and_cycle_sort() {
    let keys: Vec<&str> = SETTINGS_KEYS.iter().map(|(_, k)| *k).collect();
    assert!(keys.contains(&"keybinds.search"));
    assert!(keys.contains(&"keybinds.cycle_sort"));
}
