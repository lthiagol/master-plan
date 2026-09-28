//! The theme catalog has exactly one source of truth.
//!
//! `mp_model::UI_THEMES` is the list `mp config set ui.theme` validates
//! against and `mp config schema` advertises. `raul::theme::ALL` is
//! the list of palettes raul can actually render. This file asserts
//! the two sets are equal in both directions, so neither can drift:
//!
//!   * a name added to mp but not to raul would validate in config
//!     and then silently no-op at runtime (`Palette::by_name` → `None`)
//!   * a palette added to raul but not to mp would render fine but be
//!     impossible to select, and would be dropped by the schema the
//!     Settings picker reads its rows from
//!
//! `monochrome` is deliberately outside both: it is the no-color
//! fallback chosen by `ui.color=false`, not a `ui.theme` value.

use raul::theme::{self, Palette};

#[test]
fn theme_catalog_parity() {
    let mp: Vec<&str> = mp_model::UI_THEMES.to_vec();
    let raul: Vec<&str> = theme::ALL.iter().map(|p| p.name).collect();

    let missing_in_raul: Vec<&&str> = mp.iter().filter(|n| !raul.contains(n)).collect();
    assert!(
        missing_in_raul.is_empty(),
        "mp advertises theme name(s) raul cannot render: {missing_in_raul:?}"
    );

    let missing_in_mp: Vec<&&str> = raul.iter().filter(|n| !mp.contains(n)).collect();
    assert!(
        missing_in_mp.is_empty(),
        "raul ships palette(s) mp rejects / does not advertise: {missing_in_mp:?}"
    );

    // Neither list may carry duplicates — a dup in either would make
    // the picker render the same palette twice.
    let mut sorted_mp = mp.clone();
    sorted_mp.sort_unstable();
    sorted_mp.dedup();
    assert_eq!(
        sorted_mp.len(),
        mp.len(),
        "UI_THEMES has a duplicate: {mp:?}"
    );

    let mut sorted_raul = raul.clone();
    sorted_raul.sort_unstable();
    sorted_raul.dedup();
    assert_eq!(
        sorted_raul.len(),
        raul.len(),
        "theme::ALL has a duplicate: {raul:?}"
    );
}

/// Every advertised name must resolve through the public lookup, and
/// the default must be one of them — `Palette::default_palette()`
/// panics otherwise, and `mp`'s schema default is `mocha`.
#[test]
fn theme_catalog_names_resolve_and_default_is_catalogued() {
    for name in mp_model::UI_THEMES {
        assert!(
            Palette::by_name(name).is_some(),
            "UI_THEMES advertises {name:?} but Palette::by_name returns None"
        );
    }
    assert!(
        mp_model::UI_THEMES.contains(&Palette::DEFAULT_NAME),
        "Palette::DEFAULT_NAME ({:?}) is not in UI_THEMES",
        Palette::DEFAULT_NAME
    );
}

/// `monochrome` stays out of the user-selectable catalog on both
/// sides. It is the `ui.color=false` fallback, and listing it in the
/// picker would offer a theme that renders no color at all.
#[test]
fn monochrome_stays_outside_the_theme_catalog() {
    assert!(
        !mp_model::UI_THEMES.contains(&theme::MONOCHROME.name),
        "monochrome must not be a ui.theme value"
    );
    assert!(
        !theme::ALL.iter().any(|p| p.name == theme::MONOCHROME.name),
        "monochrome must stay out of theme::ALL"
    );
    assert_eq!(theme::ALL.len(), 6, "the catalog carries 6 named palettes");
}
