//! M185 AC-11: help entries + settings keys include F/g.

use raul::tui::keybinds::Keybinds;
use raul::tui::modes::settings::SETTINGS_KEYS;

#[test]
fn help_entries_list_lifecycle_filter_and_grooming() {
    let entries = Keybinds::default().help_entries();
    let labels: Vec<_> = entries.iter().map(|e| e.label).collect();
    assert!(
        labels.iter().any(|l| l.contains("Lifecycle filter")),
        "help must mention lifecycle filter; got {labels:?}"
    );
    assert!(
        labels.iter().any(|l| l.contains("Grooming preset")),
        "help must mention grooming preset; got {labels:?}"
    );
    assert!(
        labels
            .iter()
            .any(|l| l.contains("lowercase f") || l.contains("Toggle filter")),
        "help must note lowercase f; got {labels:?}"
    );
}

#[test]
fn settings_keys_include_new_keybinds() {
    let keys: Vec<&str> = SETTINGS_KEYS.iter().map(|(_, k)| *k).collect();
    assert!(keys.contains(&"keybinds.lifecycle_filter"));
    assert!(keys.contains(&"keybinds.grooming_preset"));
}
