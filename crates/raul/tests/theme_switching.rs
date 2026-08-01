use raul::theme::Palette;

#[test]
fn all_five_flavors_resolve_in_order() {
    let names: Vec<_> = Palette::all().iter().map(|p| p.name).collect();
    assert_eq!(
        names,
        vec!["latte", "frappe", "macchiato", "mocha", "dracula"]
    );
}

#[test]
fn by_name_resolves_known_themes() {
    assert_eq!(Palette::by_name("mocha").map(|p| p.name), Some("mocha"));
    assert_eq!(Palette::by_name("dracula").map(|p| p.name), Some("dracula"));
    assert_eq!(Palette::by_name("latte").map(|p| p.name), Some("latte"));
}

#[test]
fn by_name_rejects_unknown() {
    assert!(Palette::by_name("nord").is_none());
    assert!(Palette::by_name("").is_none());
}

#[test]
fn default_palette_is_mocha_and_resolves() {
    assert_eq!(Palette::DEFAULT_NAME, "mocha");
    let d = Palette::default_palette();
    assert_eq!(d.name, "mocha");
}

#[test]
fn flavors_have_distinct_accents() {
    let accents: Vec<_> = Palette::all().iter().map(|p| p.accent).collect();
    let mut deduped = accents.clone();
    deduped.dedup();
    assert_eq!(
        deduped.len(),
        accents.len(),
        "each flavor must have a distinct accent"
    );
}
