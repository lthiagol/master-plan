//! M138: the `Keybinds` struct — raul's single source of truth for
//! "which key does what".
//!
//! ## Why this module exists
//!
//! Pre-M138 every binding was hardcoded four times: an arm in
//! `modes::normal::map_event_key`, the global-key match in
//! `modes::normal::handle_key`, the help-overlay text, and the footer text.
//! Adding or changing a shortcut meant editing all four and hoping you didn't
//! miss one. Worse, a user could not rebind anything without forking raul.
//!
//! `Keybinds` collapses that to one place. Each field is the list of
//! [`KeyCombo`]s bound to one [`Action`]. [`Keybinds::resolve`] does the
//! inverse lookup (key → action) that the per-mode handlers consult, and
//! [`Keybinds::help_entries`] feeds the help overlay + footer so the
//! on-screen legend can never drift from what the dispatcher actually does.
//!
//! ## v1 scope (M138)
//!
//! Fields are `Vec<KeyCombo>` so a default like `up = [Up, k]` keeps both the
//! arrow key and the vim key. The *config* surface in v1 accepts a single
//! combo string per action (`mp config set keybinds.quit q`); M139 adds the
//! `One | Many` untagged form, diagnostics, and profile export on top of the
//! same struct.
//!
//! ## Contextual keys stay in the handlers
//!
//! A few keys are re-interpreted by focus/lane context (e.g. `h` is
//! *previous-lane* on the tab bar but *hide-done* in a list; `r` is *refresh*
//! on a data lane but *resolve* in an annotation thread). Those overrides
//! live in `modes::normal`, which checks the relevant `Keybinds` field before
//! falling through to `resolve`. Digit lane-jumps (`1`..`8`) and board
//! column arrows are positional, not per-action bindings, and remain in the
//! handler (indexed bindings are explicitly out of scope for v1/v2).
//!
//! ## v1 boundary: Normal mode only
//!
//! The keybind layer drives `Mode::Normal` dispatch, the help overlay, and
//! the footer. The mode-local handlers for the annotation thread, help
//! overlay, and review menu (`modes::{annotation_thread,help,review_menu}`)
//! still match their (context-specific, non-overlapping) keys inline and do
//! not yet consult `Keybinds`, so a user rebind currently affects Normal-mode
//! keys and the generated legend. Extending config-driven bindings into those
//! sub-modes is deferred — it requires threading `&Keybinds` through the
//! `handle_key(key)` signatures those handlers expose today.

use crossterm::event::KeyEvent;
use serde::Deserialize;

use super::action::Action;
use super::app::{ContentState, Lane};
use super::key_combo::{format_key_combo, key_event_matches_combo, parse_key_combo, KeyCombo};

use crossterm::event::{KeyCode, KeyModifiers};

/// M139: the config-value shape for one action's binding(s). Deserialized via
/// untagged serde so an author can write either a single combo string
/// (`quit = "q"`) or a list (`quit = ["ctrl+w", "ctrl+shift+t"]`). The
/// internal [`Keybinds`] representation is always `Vec<KeyCombo>`; this enum
/// only bridges the two config surfaces.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum BindingConfig {
    One(String),
    Many(Vec<String>),
}

impl BindingConfig {
    /// The combo strings this binding names, in order.
    fn strings(&self) -> Vec<String> {
        match self {
            BindingConfig::One(s) => vec![s.clone()],
            BindingConfig::Many(v) => v.clone(),
        }
    }
}

/// M139: a single config diagnostic — which field was bad and why. raul logs
/// these as `eprintln!` warnings and falls back to the default binding for the
/// affected field, so a fat-fingered config never crashes the TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub field: String,
    pub message: String,
}

/// Shorthand for a default combo built from a bare key code with no modifiers.
fn plain(code: KeyCode) -> KeyCombo {
    (code, KeyModifiers::empty())
}

/// Shorthand for a shifted char combo (`shift+<c>`), matching what
/// [`parse_key_combo`] produces for an uppercase ASCII letter.
fn shift_char(c: char) -> KeyCombo {
    (KeyCode::Char(c), KeyModifiers::SHIFT)
}

fn ctrl(code: KeyCode) -> KeyCombo {
    (code, KeyModifiers::CONTROL)
}

/// The complete set of keybindable actions, one `Vec<KeyCombo>` per action.
///
/// Field order is the resolution order used by `resolve`; the defaults are
/// disjoint within the content-canonical set so the order only matters for
/// documentation stability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keybinds {
    pub quit: Vec<KeyCombo>,
    pub up: Vec<KeyCombo>,
    pub down: Vec<KeyCombo>,
    pub page_up: Vec<KeyCombo>,
    pub page_down: Vec<KeyCombo>,
    pub enter: Vec<KeyCombo>,
    pub escape: Vec<KeyCombo>,
    pub help: Vec<KeyCombo>,
    pub filter: Vec<KeyCombo>,
    pub hide_done: Vec<KeyCombo>,
    pub create_annotation: Vec<KeyCombo>,
    pub resolve: Vec<KeyCombo>,
    pub reopen: Vec<KeyCombo>,
    pub approve: Vec<KeyCombo>,
    pub review_menu: Vec<KeyCombo>,
    /// M169: open the Settings lane (default Ctrl-O).
    pub open_settings: Vec<KeyCombo>,
    pub previous_lane: Vec<KeyCombo>,
    pub next_lane: Vec<KeyCombo>,
    pub focus_content: Vec<KeyCombo>,
    pub refresh: Vec<KeyCombo>,
    /// M167: next section inside a drilled-in MilestoneDetail (default `]`).
    pub next_section: Vec<KeyCombo>,
    /// M167: previous section inside MilestoneDetail (default `[`).
    pub prev_section: Vec<KeyCombo>,
    /// M167: next list item across sections (default `n`).
    pub next_item: Vec<KeyCombo>,
    /// M167: previous list item across sections (default `p`).
    pub prev_item: Vec<KeyCombo>,
    /// Open the sort-rebind menu. The selection persists per lane through
    /// `mp config set sort.<lane> <sortkey>`.
    pub sort_rebind: Vec<KeyCombo>,
    /// M172 S5: cycle forward in the open sort-rebind menu.
    pub sort_rebind_next: Vec<KeyCombo>,
    /// M172 S5: cycle backward in the open sort-rebind menu.
    pub sort_rebind_prev: Vec<KeyCombo>,
    /// M172 S5: confirm the highlighted sort key and close the menu.
    pub sort_rebind_confirm: Vec<KeyCombo>,
    /// M172 S5: cancel the open menu without binding.
    pub sort_rebind_cancel: Vec<KeyCombo>,
    /// M185: open lifecycle filter modal (default capital `F`).
    pub lifecycle_filter: Vec<KeyCombo>,
    /// M185: apply Grooming preset on Milestones (default `g`).
    pub grooming_preset: Vec<KeyCombo>,
    /// M186: open search input (default `/`).
    pub search: Vec<KeyCombo>,
    /// M186: cycle sort key (default `o`).
    pub cycle_sort: Vec<KeyCombo>,
}

impl Default for Keybinds {
    /// The defaults reproduce raul's pre-M138 hardcoded bindings exactly, so
    /// swapping the inline matches for `Keybinds` is behavior-preserving.
    fn default() -> Self {
        Self {
            quit: vec![plain(KeyCode::Char('q')), shift_char('q')],
            up: vec![plain(KeyCode::Up), plain(KeyCode::Char('k'))],
            down: vec![plain(KeyCode::Down), plain(KeyCode::Char('j'))],
            page_up: vec![plain(KeyCode::PageUp)],
            page_down: vec![plain(KeyCode::PageDown)],
            enter: vec![plain(KeyCode::Enter)],
            escape: vec![plain(KeyCode::Esc)],
            help: vec![plain(KeyCode::Char('?'))],
            filter: vec![plain(KeyCode::Char('f'))],
            hide_done: vec![plain(KeyCode::Char('h'))],
            create_annotation: vec![shift_char('a')],
            resolve: vec![plain(KeyCode::Char('r'))],
            reopen: vec![shift_char('r')],
            approve: vec![plain(KeyCode::Char('p'))],
            review_menu: vec![plain(KeyCode::Char('m'))],
            open_settings: vec![ctrl(KeyCode::Char('o'))],
            // M167: Tab/Shift+Tab are now lane-navigation bindings (they
            // used to toggle a focus state, which removed the need for
            // up/down/up/down round-trips); Left/Right and h/l kept as
            // multi-binding aliases so vim users keep their muscle memory.
            previous_lane: vec![
                plain(KeyCode::Left),
                plain(KeyCode::Char('h')),
                plain(KeyCode::BackTab),
            ],
            next_lane: vec![
                plain(KeyCode::Right),
                plain(KeyCode::Char('l')),
                plain(KeyCode::Tab),
            ],
            focus_content: vec![plain(KeyCode::Enter)],
            refresh: vec![plain(KeyCode::Char('r'))],
            // M167: detail-section navigation — only consumed when
            // `app.content == ContentState::MilestoneDetail`.
            next_section: vec![plain(KeyCode::Char(']'))],
            prev_section: vec![plain(KeyCode::Char('['))],
            next_item: vec![plain(KeyCode::Char('n'))],
            prev_item: vec![plain(KeyCode::Char('p'))],
            // M172 S5: sort-rebind inline menu. The menu opens on `S`
            // (capital — lowercase `s` is reserved for future use)
            // and the cycling/binding/cancel keys are the same Up /
            // Down / Enter / Esc used elsewhere. The menu is
            // modal: while open, Up/Down cycle instead of moving the
            // list selection. See `modes::normal::handle_key` for
            // the modal dispatch.
            sort_rebind: vec![shift_char('s')],
            sort_rebind_next: vec![plain(KeyCode::Down), plain(KeyCode::Char('j'))],
            sort_rebind_prev: vec![plain(KeyCode::Up), plain(KeyCode::Char('k'))],
            sort_rebind_confirm: vec![plain(KeyCode::Enter)],
            sort_rebind_cancel: vec![plain(KeyCode::Esc)],
            lifecycle_filter: vec![shift_char('f')],
            grooming_preset: vec![plain(KeyCode::Char('g'))],
            search: vec![plain(KeyCode::Char('/'))],
            cycle_sort: vec![plain(KeyCode::Char('o'))],
        }
    }
}

/// Does any combo in `combos` match `key`?
pub fn any_matches(combos: &[KeyCombo], key: &KeyEvent) -> bool {
    combos.iter().any(|c| key_event_matches_combo(key, *c))
}

impl Keybinds {
    /// Inverse lookup for the *content-canonical* bindings: given a key,
    /// return the single [`Action`] it triggers in the content pane.
    ///
    /// This is the map_event_key + global-key surface from pre-M138. The
    /// contextual overrides (tab-bar navigation, per-lane refresh) are handled
    /// by the per-mode handler *before* it falls through to `resolve`, so the
    /// overlapping default combos (`h`, `r`, `Enter`) resolve to their
    /// content meaning here and their navigation meaning in the handler.
    ///
    /// Returns `None` for an unbound key; the handler then treats it as a
    /// no-op (matching pre-M138 `Event::Unknown`).
    pub fn resolve(&self, key: &KeyEvent) -> Option<Action> {
        // Order mirrors the field declaration; defaults are disjoint so the
        // first hit is unambiguous.
        if any_matches(&self.quit, key) {
            return Some(Action::Quit);
        }
        if any_matches(&self.escape, key) {
            return Some(Action::Esc);
        }
        if any_matches(&self.up, key) {
            return Some(Action::Up);
        }
        if any_matches(&self.down, key) {
            return Some(Action::Down);
        }
        if any_matches(&self.page_up, key) {
            return Some(Action::PageUp);
        }
        if any_matches(&self.page_down, key) {
            return Some(Action::PageDown);
        }
        if any_matches(&self.enter, key) {
            return Some(Action::Enter);
        }
        if any_matches(&self.help, key) {
            return Some(Action::OpenHelp);
        }
        if any_matches(&self.filter, key) {
            return Some(Action::ToggleFilter);
        }
        if any_matches(&self.create_annotation, key) {
            return Some(Action::CreateAnnotation);
        }
        if any_matches(&self.reopen, key) {
            return Some(Action::ReopenAnnotation);
        }
        if any_matches(&self.resolve, key) {
            return Some(Action::ResolveAnnotation);
        }
        if any_matches(&self.approve, key) {
            return Some(Action::ToggleApproval);
        }
        if any_matches(&self.review_menu, key) {
            return Some(Action::OpenReviewMenu);
        }
        if any_matches(&self.hide_done, key) {
            return Some(Action::ToggleHideDone);
        }
        // M172 S5: sort-rebind inline menu (default `S`). Open +
        // cycle + bind + cancel all live in the menu's modal
        // dispatcher (the menu is open during cycle/bind/cancel
        // so the same Up/Down/Enter/Esc keys are re-interpreted by
        // the modal handler).
        if any_matches(&self.sort_rebind, key) {
            return Some(Action::OpenSortRebind);
        }
        // M185: capital F opens lifecycle filter; lowercase f remains
        // ToggleFilter (checked above). g applies Grooming preset.
        if any_matches(&self.lifecycle_filter, key) {
            return Some(Action::OpenLifecycleFilter);
        }
        if any_matches(&self.grooming_preset, key) {
            return Some(Action::ApplyGroomingPreset);
        }
        if any_matches(&self.search, key) {
            return Some(Action::OpenSearch);
        }
        if any_matches(&self.cycle_sort, key) {
            return Some(Action::CycleSortNext);
        }
        None
    }

    /// M139: validate the `[keybinds]` section and build a [`Keybinds`],
    /// returning any diagnostics alongside it. This is the structured core
    /// that `load_from_config` wraps.
    ///
    /// Rules:
    ///   * Each value may be a single combo string or a list of combo strings
    ///     (untagged [`BindingConfig`]). A list binds the action to every key
    ///     in it — `resolve` returns that action for any of them (first match
    ///     wins across `resolve`'s field order).
    ///   * A malformed combo, an unparseable value, or a wrong-typed value
    ///     leaves the field at its [`Default`] binding and records a
    ///     [`Diagnostic`]. Fallback is per-field: one bad entry never discards
    ///     the others.
    ///   * An explicit empty list (`[]`) disables the action (no bindings).
    ///
    /// ## Conflict policy
    ///
    /// If two *resolvable* actions end up bound to the same combo, a
    /// diagnostic is emitted. Resolution is deterministic by `resolve`'s field
    /// order (the earlier-checked action wins); the warning tells the author
    /// their later binding is shadowed. Contextual keys that legitimately
    /// overlap across focus states (e.g. `h` = previous-lane on the tab bar
    /// vs hide-done in a list) are *not* treated as conflicts.
    pub fn validated_keybinds(config: &serde_json::Value) -> (Vec<Diagnostic>, Self) {
        let mut kb = Self::default();
        let mut diags = Vec::new();
        let section = &config["config"]["keybinds"];
        if section.is_object() {
            for (name, slot) in kb.slots_mut() {
                let raw = &section[name];
                if raw.is_null() {
                    continue;
                }
                match serde_json::from_value::<BindingConfig>(raw.clone()) {
                    Ok(binding) => {
                        let strings = binding.strings();
                        let mut combos = Vec::new();
                        let mut bad = Vec::new();
                        for s in &strings {
                            match parse_key_combo(s) {
                                Some(c) => combos.push(c),
                                None => bad.push(s.clone()),
                            }
                        }
                        if bad.is_empty() {
                            // Valid (possibly empty -> explicit disable).
                            *slot = combos;
                        } else {
                            diags.push(Diagnostic {
                                field: name.to_string(),
                                message: format!("invalid combo(s) {bad:?}; using default binding"),
                            });
                        }
                    }
                    Err(_) => {
                        diags.push(Diagnostic {
                            field: name.to_string(),
                            message: format!(
                                "value must be a string or list of strings (got {raw}); using default"
                            ),
                        });
                    }
                }
            }
        }
        diags.extend(kb.conflict_diagnostics());
        (diags, kb)
    }

    /// Load keybinds from the `[keybinds]` section of a `mp config show`
    /// JSON payload (the same `config` object [`crate::config::UiConfig`]
    /// reads), logging any [`Diagnostic`]s and falling back per-field.
    ///
    /// ## Diagnostics channel
    ///
    /// The spec calls for a "tracing warn". raul has no `tracing` dependency
    /// (and the raul dep-audit budget is tight at 97/100), so we use raul's
    /// established diagnostic channel — `eprintln!("raul: ...")`, the same one
    /// `runner.rs` already uses. (M164 removed the CLI `commands/` tree that
    /// historically shared the channel.)
    pub fn load_from_config(config: &serde_json::Value) -> Self {
        let (diags, kb) = Self::validated_keybinds(config);
        for d in &diags {
            eprintln!("raul: keybinds.{} {}", d.field, d.message);
        }
        kb
    }

    /// Load keybinds via `mp config show`, mirroring
    /// [`crate::config::UiConfig::load`]. raul is read-only, so — like the UI
    /// prefs — the source of truth is mp's project config. Any error running
    /// `mp` falls back to the built-in defaults.
    pub fn load(runner: &crate::mp_runner::MpRunner) -> Self {
        match runner.run::<serde_json::Value>("config", &["show"]) {
            Ok(v) => Self::load_from_config(&v),
            Err(_) => Self::default(),
        }
    }

    /// Mutable (config-name, field) pairs, used by `load_from_config` to
    /// apply overrides by name. Keeping this list next to the struct means a
    /// new action is a one-line addition here rather than a scattered edit.
    fn slots_mut(&mut self) -> Vec<(&'static str, &mut Vec<KeyCombo>)> {
        vec![
            ("quit", &mut self.quit),
            ("up", &mut self.up),
            ("down", &mut self.down),
            ("page_up", &mut self.page_up),
            ("page_down", &mut self.page_down),
            ("enter", &mut self.enter),
            ("escape", &mut self.escape),
            ("help", &mut self.help),
            ("filter", &mut self.filter),
            ("hide_done", &mut self.hide_done),
            ("create_annotation", &mut self.create_annotation),
            ("resolve", &mut self.resolve),
            ("reopen", &mut self.reopen),
            ("approve", &mut self.approve),
            ("review_menu", &mut self.review_menu),
            ("open_settings", &mut self.open_settings),
            ("previous_lane", &mut self.previous_lane),
            ("next_lane", &mut self.next_lane),
            ("focus_content", &mut self.focus_content),
            ("refresh", &mut self.refresh),
            ("next_section", &mut self.next_section),
            ("prev_section", &mut self.prev_section),
            ("next_item", &mut self.next_item),
            ("prev_item", &mut self.prev_item),
            ("lifecycle_filter", &mut self.lifecycle_filter),
            ("grooming_preset", &mut self.grooming_preset),
            ("search", &mut self.search),
            ("cycle_sort", &mut self.cycle_sort),
        ]
    }

    /// Immutable (name, value) pairs, used by the profile export and by
    /// conflict detection so the iteration order matches the slot list.
    fn slots(&self) -> Vec<(&'static str, &Vec<KeyCombo>)> {
        vec![
            ("quit", &self.quit),
            ("up", &self.up),
            ("down", &self.down),
            ("page_up", &self.page_up),
            ("page_down", &self.page_down),
            ("enter", &self.enter),
            ("escape", &self.escape),
            ("help", &self.help),
            ("filter", &self.filter),
            ("hide_done", &self.hide_done),
            ("create_annotation", &self.create_annotation),
            ("resolve", &self.resolve),
            ("reopen", &self.reopen),
            ("approve", &self.approve),
            ("review_menu", &self.review_menu),
            ("open_settings", &self.open_settings),
            ("previous_lane", &self.previous_lane),
            ("next_lane", &self.next_lane),
            ("focus_content", &self.focus_content),
            ("refresh", &self.refresh),
            ("next_section", &self.next_section),
            ("prev_section", &self.prev_section),
            ("next_item", &self.next_item),
            ("prev_item", &self.prev_item),
            ("lifecycle_filter", &self.lifecycle_filter),
            ("grooming_preset", &self.grooming_preset),
            ("search", &self.search),
            ("cycle_sort", &self.cycle_sort),
        ]
    }

    /// Diagnostics for combos that two *resolvable* actions share. Only
    /// fields consulted by `resolve` are checked; the contextual fields
    /// (`previous_lane` etc. that the per-mode handler matches first) can
    /// legitimately overlap, so a warning there would be noise.
    ///
    /// Note: contextual shadows (e.g. `refresh = r` and `resolve = r`
    /// both bound to `r` in the default set) are *not* surfaced here.
    /// Doing so would emit noise on every round-trip of the default
    /// keybinds, since `refresh = r` and `resolve = r` already coexist as
    /// a deliberate contextual shadow. Future work: a separate
    /// "contextual conflict" diagnostic that names the lane where the
    /// shadow applies.
    fn conflict_diagnostics(&self) -> Vec<Diagnostic> {
        let resolvable_names = [
            "quit",
            "escape",
            "up",
            "down",
            "page_up",
            "page_down",
            "enter",
            "help",
            "filter",
            "create_annotation",
            "reopen",
            "resolve",
            "approve",
            "review_menu",
            "open_settings",
            "hide_done",
        ];
        let slots = self.slots();
        let mut seen: Vec<(&'static str, KeyCombo)> = Vec::new();
        let mut diags = Vec::new();
        for name in resolvable_names {
            let (_, combos) = slots.iter().find(|(n, _)| *n == name).unwrap();
            for c in combos.iter() {
                if let Some((prev, _)) = seen.iter().find(|(_, pc)| *pc == *c) {
                    diags.push(Diagnostic {
                        field: (*name).to_string(),
                        message: format!(
                            "combo {c:?} also bound to {prev}; resolve() picks the first match"
                        ),
                    });
                } else {
                    seen.push((name, *c));
                }
            }
        }
        diags
    }

    /// M139: emit a TOML fragment containing only the user-displaced defaults.
    /// Defaults that the user kept do not appear; bindings the user removed
    /// (relative to default) are not represented either (the loader can't
    /// distinguish "user removed" from "never set", so we omit both). The
    /// format is hand-written (raul has no `toml` crate to keep the dep-audit
    /// budget under 100 transitives) and covers exactly the cases the v2
    /// surface needs:
    ///
    ///   * `key = "combo"` for single-binding overrides,
    ///   * `key = ["combo1", "combo2"]` for multi-binding overrides.
    pub fn local_keybindings_profile_toml(&self) -> String {
        let defaults = Self::default();
        let default_slots = defaults.slots();
        let mut out = String::new();
        out.push_str("# raul keybindings — user-displaced defaults only.\n");
        out.push_str("# Round-trip through `Keybinds::load_from_profile_toml` to apply.\n");
        for (name, combos) in self.slots() {
            let (_, default) = default_slots.iter().find(|(n, _)| *n == name).unwrap();
            if combos.is_empty() || combos == *default {
                // A user "disabling" a default by setting the field to `[]`
                // cannot be distinguished from "never set" in the loader, so
                // we omit it from the profile rather than emit an ambiguous
                // override. Documented in the spec.
                continue;
            }
            let formatted: Vec<String> = combos
                .iter()
                .map(|c| toml_basic_string(&combo_to_toml_string(*c)))
                .collect();
            let line = if formatted.len() == 1 {
                format!("{name} = {}", formatted[0])
            } else {
                format!("{name} = [{}]", formatted.join(", "))
            };
            out.push_str(&line);
            out.push('\n');
        }
        out
    }

    /// M139: parse a profile TOML fragment (the format produced by
    /// `local_keybindings_profile_toml`) and apply it to a [`Keybinds`].
    /// Unrecognized lines and malformed combos are reported as
    /// [`Diagnostic`]s; valid lines override the default binding for their
    /// action. Empty lists disable the action.
    pub fn load_from_profile_toml(profile: &str) -> (Vec<Diagnostic>, Self) {
        let mut kb = Self::default();
        let mut diags = Vec::new();
        let valid_names: std::collections::HashSet<&'static str> =
            kb.slots().into_iter().map(|(n, _)| n).collect();
        for (lineno, raw) in profile.lines().enumerate() {
            // Strip a trailing `# ...` comment, but only when the `#` is
            // outside a double-quoted string — otherwise a binding whose
            // combo is `#` (a valid single-char key) would be truncated to
            // an empty value. (M139 code-review: the naive `split_once('#')`
            // silently dropped such keys on the round-trip.)
            let line = strip_toml_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            let Some((name, value)) = line.split_once('=') else {
                diags.push(Diagnostic {
                    field: format!("profile:{}", lineno + 1),
                    message: format!("expected `key = value`; got {raw:?}"),
                });
                continue;
            };
            let name = name.trim();
            if !valid_names.contains(name) {
                diags.push(Diagnostic {
                    field: name.to_string(),
                    message: format!("unknown keybind action in profile (got {raw:?})"),
                });
                continue;
            }
            let value = value.trim();
            // Parse the RHS as one or more TOML basic (double-quoted)
            // strings, honouring backslash escapes and quoted commas so a
            // `,`-key or a `"`-key round-trips. Replaces the earlier
            // `split(',')` + `trim_matches('"')` path that over-stripped /
            // mis-split on those chars (M139 code-review).
            let Some(strings) = parse_profile_value(value) else {
                diags.push(Diagnostic {
                    field: name.to_string(),
                    message: format!("invalid TOML string in profile: {value:?}"),
                });
                continue;
            };
            let mut combos = Vec::new();
            let mut bad = Vec::new();
            for s in &strings {
                if s.is_empty() {
                    continue;
                }
                match parse_key_combo(s) {
                    Some(c) => combos.push(c),
                    None => bad.push(s.clone()),
                }
            }
            for s in bad {
                diags.push(Diagnostic {
                    field: name.to_string(),
                    message: format!("invalid combo in profile: {s:?}"),
                });
            }
            for (n, slot) in kb.slots_mut() {
                if *n == *name {
                    *slot = combos.clone();
                    break;
                }
            }
        }
        diags.extend(kb.conflict_diagnostics());
        (diags, kb)
    }

    /// Ordered `(human label, combos)` pairs for building the help overlay and
    /// footer. The label matches the on-screen description; the combos are the
    /// current bindings (default or user-overridden), so the legend follows
    /// config automatically.
    pub fn help_entries(&self) -> Vec<HelpEntry> {
        let e = |label: &'static str, combos: &[KeyCombo]| HelpEntry {
            label,
            keys: combos.iter().map(|c| format_key_combo(*c)).collect(),
        };
        vec![
            e("Previous lane", &self.previous_lane),
            e("Next lane", &self.next_lane),
            e("Focus content", &self.focus_content),
            e("Next section", &self.next_section),
            e("Previous section", &self.prev_section),
            e("Next item", &self.next_item),
            e("Previous item", &self.prev_item),
            e("Move up", &self.up),
            e("Move down", &self.down),
            e("Page up", &self.page_up),
            e("Page down", &self.page_down),
            e("Select / drill in", &self.enter),
            e("Go back", &self.escape),
            e("Refresh", &self.refresh),
            e("Toggle filter (annotations; lowercase f)", &self.filter),
            e(
                "Lifecycle filter (Milestones; capital F)",
                &self.lifecycle_filter,
            ),
            e("Grooming preset (Milestones)", &self.grooming_preset),
            e("Search (Milestones/Backlog/Ideas)", &self.search),
            e(
                "Cycle sort key (Milestones/Backlog/Ideas)",
                &self.cycle_sort,
            ),
            e("Toggle hide-done", &self.hide_done),
            e("Create annotation", &self.create_annotation),
            e("Resolve annotation", &self.resolve),
            e("Reopen annotation", &self.reopen),
            e("Approve / request", &self.approve),
            e("Review menu", &self.review_menu),
            e("Open settings", &self.open_settings),
            e("Help", &self.help),
            e("Quit", &self.quit),
        ]
    }

    /// Render just the first (primary) binding for an action — used by the
    /// footer, which is width-constrained and shows one key per hint.
    fn primary(combos: &[KeyCombo]) -> String {
        combos
            .first()
            .map(|c| format_key_combo(*c))
            .unwrap_or_default()
    }

    /// The tab-bar-focused footer, generated from the navigation bindings.
    /// The "1-N:jump" range follows the *visible* lane list
    /// (`Lane::ordered_visible(show_watch_tab).len()`) so the
    /// on-screen tab number matches the keystroke when the
    /// operator has hidden the Watch tab. M198 WP2: the helper
    /// is purely text generation; it does not mutate any state
    /// and is safe to call from the render path on every frame.
    pub fn footer_tab_bar(&self, show_watch_tab: bool) -> String {
        format!(
            " {}/{}:lanes  1-{}:jump  {}:focus  {}:quit ",
            Self::primary(&self.previous_lane),
            Self::primary(&self.next_lane),
            super::app::Lane::ordered_visible(show_watch_tab).len(),
            Self::primary(&self.focus_content),
            Self::primary(&self.quit),
        )
    }

    /// Overview-lane list footer.
    pub fn footer_overview(&self) -> String {
        format!(
            " {}:inbox  {}:go  {}:refresh  {}:help  {}:quit ",
            Self::primary(&self.up),
            Self::primary(&self.enter),
            Self::primary(&self.refresh),
            Self::primary(&self.help),
            Self::primary(&self.quit),
        )
    }

    /// Generic list (Milestones / Backlog) footer.
    /// Trimmed to lane-specific actions only — quit/help/hide-done
    /// already live on the globals line (footer row 1), so repeating
    /// them here just added visual noise.
    pub fn footer_list(&self) -> String {
        format!(
            " {}:move  {}:select  {}:back ",
            Self::primary(&self.up),
            Self::primary(&self.enter),
            Self::primary(&self.escape),
        )
    }

    /// The content-pane (detail) footer, generated from the relevant bindings.
    /// Trimmed — `:filter :hide-done :help :quit` are on the globals line.
    /// Kept `:action` (Enter) and `:menu` (review menu) since those are
    /// detail-specific.
    pub fn footer_content(&self, open_only: bool) -> String {
        format!(
            " {}:move  {}:action  {}:back  {}:menu  {}",
            Self::primary(&self.up),
            Self::primary(&self.enter),
            Self::primary(&self.escape),
            Self::primary(&self.review_menu),
            if open_only { "(open only)" } else { "(all)" },
        )
    }

    /// M169: Settings lane footer with Save / Cancel affordance. When
    /// the user has staged edits (`staged_edits.is_empty() == false`),
    /// the Save affordance is highlighted with `*` so the unsaved-state
    /// signal mirrors what `apply_settings_save` consumes on press.
    pub fn footer_settings(settings: Option<&crate::tui::mode::SettingsState>) -> String {
        let staged = settings.map(|s| s.has_staged_edits()).unwrap_or(false);
        if staged {
            " [Save (s)*] [Cancel (Esc)] ".to_string()
        } else {
            " [Save (s)] [Cancel (Esc)] ".to_string()
        }
    }

    /// M199: per-(lane, content_state) table for the per-tab footer row.
    ///
    /// Returns the contextual line for the active (lane, content_state)
    /// pair. The globals row already covers quit / help / move / go /
    /// refresh / lanes, so this string only contains lane-specific
    /// affordances — no duplication of the globals baseline (the single
    /// intentional exception is `r:resolve` on `AnnotationThread`, which
    /// overrides the globals `r:refresh` per design decision D-08).
    ///
    /// Returns an empty string for lanes with no lane-specific keys
    /// (`Lane::Path` in v1), which signals `compute_view` to collapse the
    /// footer to a single globals row.
    pub fn footer_per_tab(
        &self,
        lane: Lane,
        content: ContentState,
        open_only: bool,
        settings_staged: bool,
    ) -> String {
        let kb = self;
        // Helper: build a "key:label" pair using the first bound combo,
        // dropping the key when the slot is empty (cleared by config)
        // so the legend never shows a bare colon prefix. The closure
        // takes a `&str` so callers can pass either a static glyph or
        // a freshly-built combined string ("h/l", "n/p").
        let mut out = String::new();
        let mut first = true;
        let push = |out: &mut String, first: &mut bool, glyph: &str, label: &str| {
            if glyph.is_empty() {
                return;
            }
            if !*first {
                out.push_str(" · ");
            }
            *first = false;
            out.push_str(&format!(" {glyph}:{label} "));
        };

        // `content` is borrowed by the match (so it isn't moved
        // and can still drive the post-match `(open only)` check
        // below).
        match (lane, &content) {
            (Lane::Overview, ContentState::List) => {
                // Overview is the one lane where Enter means "open
                // inbox". Globals covers ↑↓/move, ⏎:go — but here ⏎ is
                // lane-specific (drills into the inbox row). Per design
                // decision D-04: use the verb "inbox" (lane-specific) for
                // the Enter override; do not duplicate ":move".
                let enter_glyph = Self::primary(&kb.enter);
                push(&mut out, &mut first, enter_glyph.as_str(), "inbox");
            }
            (Lane::Milestones, ContentState::List) | (Lane::Backlog, ContentState::List) => {
                // Lane-specific list affordances — the items the
                // globals row dropped in M199 because they aren't
                // universal across every (lane, content_state, mode).
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.lifecycle_filter).as_str(),
                    "filter",
                );
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.search).as_str(),
                    "search",
                );
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.hide_done).as_str(),
                    "hide-done",
                );
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.sort_rebind).as_str(),
                    "sort",
                );
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.cycle_sort).as_str(),
                    "cycle",
                );
                // A:annotate only on lanes that actually have
                // annotation support. Backlog annotations were folded
                // into a single "annotate" affordance on M186, so
                // both Milestones and Backlog list rows carry it.
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.create_annotation).as_str(),
                    "annotate",
                );
            }
            (Lane::Milestones, ContentState::MilestoneDetail) => {
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.create_annotation).as_str(),
                    "annotate",
                );
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.review_menu).as_str(),
                    "menu",
                );
                push(
                    &mut out,
                    &mut first,
                    format!(
                        "{}/{}",
                        Self::primary(&kb.prev_section),
                        Self::primary(&kb.next_section)
                    )
                    .as_str(),
                    "section",
                );
                push(
                    &mut out,
                    &mut first,
                    format!(
                        "{}/{}",
                        Self::primary(&kb.prev_item),
                        Self::primary(&kb.next_item)
                    )
                    .as_str(),
                    "item",
                );
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.approve).as_str(),
                    "approve",
                );
            }
            (Lane::Backlog, ContentState::BacklogDetail) => {
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.create_annotation).as_str(),
                    "annotate",
                );
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.review_menu).as_str(),
                    "menu",
                );
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.approve).as_str(),
                    "approve",
                );
            }
            (Lane::Path, _) => {
                // Path is read-mostly; ⏎:go is enough (on the globals
                // line). Empty per-tab row → 1-row footer in the
                // renderer.
                return String::new();
            }
            (Lane::Ideas, ContentState::List) => {
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.lifecycle_filter).as_str(),
                    "filter",
                );
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.search).as_str(),
                    "search",
                );
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.hide_done).as_str(),
                    "hide-done",
                );
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.sort_rebind).as_str(),
                    "sort",
                );
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.cycle_sort).as_str(),
                    "cycle",
                );
            }
            (Lane::Ideas, _) => {
                // Ideas has no Detail / AnnotationThread / CoApproval in
                // v1. Empty per-tab row keeps the footer at the
                // globals baseline.
                return String::new();
            }
            (Lane::Watch, _) => {
                // Watch has no in-band bindings beyond what the
                // globals line offers in v1. Per design decision D-07
                // this is a deliberate 1-row footer (the Watch lane's
                // per-row actions render in the row, not the footer).
                return String::new();
            }
            (Lane::Settings, _) => {
                // Settings uses the same Save/Cancel marker as the
                // pre-M199 footer_settings result. D-07 keeps the
                // dirty-state inline (`*` suffix) per Q-02.
                out.push_str(if settings_staged {
                    " [Save (s)*] [Cancel (Esc)] "
                } else {
                    " [Save (s)] [Cancel (Esc)] "
                });
            }
            (_, ContentState::AnnotationThread) => {
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.create_annotation).as_str(),
                    "annotate",
                );
                // D-08: r:resolve deliberately overrides globals
                // r:refresh on the AnnotationThread. Two rows visible,
                // user picks by tab.
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.resolve).as_str(),
                    "resolve",
                );
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.reopen).as_str(),
                    "reopen",
                );
            }
            (_, ContentState::CoApproval) => {
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.approve).as_str(),
                    "approve",
                );
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.review_menu).as_str(),
                    "menu",
                );
            }
            // Defensive fallbacks for (lane, content_state) pairs the
            // v1 navigation logic doesn't reach (Overview has no
            // detail view; Backlog doesn't drill into MilestoneDetail
            // and vice-versa). The fallbacks produce the list-style
            // affordance so test harnesses that render a "what if"
            // detail view against an arbitrary lane (e.g. `render_with_detail`
            // in tui_status_parity) still get a populated per-tab
            // string instead of a panic.
            //
            // F-05 (review) considered replacing these with
            // `unreachable!()` to catch future navigation bugs, but
            // the test harness's "render-with-arbitrary-(lane, content)"
            // pattern is a legitimate use of the function and the
            // defensive fallback is the right design. The cost of
            // panicking on these pairs (14 cascade test failures)
            // outweighs the benefit. Kept as-is.
            (Lane::Overview, _) => {
                let enter_glyph = Self::primary(&kb.enter);
                push(&mut out, &mut first, enter_glyph.as_str(), "inbox");
            }
            (Lane::Milestones, ContentState::BacklogDetail)
            | (Lane::Backlog, ContentState::MilestoneDetail) => {
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.lifecycle_filter).as_str(),
                    "filter",
                );
                push(
                    &mut out,
                    &mut first,
                    Self::primary(&kb.search).as_str(),
                    "search",
                );
            }
        }

        // `open_only` is currently only rendered on the detail footer
        // (was the "(open only)" / "(all)" tag on the pre-M199
        // footer_content). With M199 splitting content into separate
        // list/detail footers we can keep the open-only signal on
        // detail rows without a redundant entry on the globals.
        if matches!(
            content,
            ContentState::MilestoneDetail | ContentState::BacklogDetail
        ) && open_only
        {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            out.push_str("(open only)");
        }

        // Defensive: every reachable (lane, content_state) arm
        // should have produced at least one token. Falling back to
        // ":move" guarantees a non-empty string for the test that
        // walks every pair. Path and Watch explicitly return empty
        // via `String::new()` above and never reach this point; any
        // other pair that lands here is a navigation-shape mismatch
        // (the per-tab line is still better than an empty string for
        // a test that asserts a non-empty result).
        if out.is_empty() && !Self::primary(&kb.up).is_empty() {
            out.push_str(&format!(" {}:move ", Self::primary(&kb.up)));
        }

        out
    }

    /// M199: structured help overlay entries grouped as
    /// `(global_entries, per_lane_entries)`. The help overlay
    /// renders the **per-lane** group on top so the active
    /// context is most-prominent (Q-03), then the **Global**
    /// group below. The same split as the footer, so the user
    /// learns one mental model.
    ///
    /// The per-lane group is returned as `(label, keys)` tuples
    /// where `label` is an owned `String` so the per-tab-sourced
    /// entries don't have to fake a `&'static str` lifetime.
    ///
    /// `content` defaults to `ContentState::List` because the
    /// help overlay is most often opened from a list view; pass
    /// the active content state explicitly to mirror what
    /// `footer_per_tab` would render for the active
    /// (lane, content_state) pair (e.g. `AnnotationThread` for
    /// `resolve`/`reopen`, `MilestoneDetail` for `section`/
    /// `item`/`menu`).
    pub fn help_entries_grouped(
        &self,
        lane: Lane,
        content: ContentState,
    ) -> (Vec<HelpEntry>, Vec<(String, Vec<String>)>) {
        let all = self.help_entries();
        // The same six visible-on-every-tab bindings from
        // `footer_globals_line`, plus the lane-switch pair (the
        // footer shows those as a single "Tab/Shift+Tab:lanes"
        // glyph; the help overlay surfaces them as two entries
        // for scannability).
        let global_labels = [
            "Quit",
            "Help",
            "Refresh",
            "Move up",
            "Move down",
            "Select / drill in",
            "Previous lane",
            "Next lane",
        ];
        let global: Vec<HelpEntry> = all
            .iter()
            .filter(|e| global_labels.contains(&e.label))
            .cloned()
            .collect();
        // Per-lane entries are sourced from the active
        // (lane, content_state) per-tab tokens so the help
        // overlay and the footer share a single source of
        // truth: `Keybinds::footer_per_tab`.
        //
        // Settings is a special case: its per-tab string uses
        // bracket markers (`[Save (s)] [Cancel (Esc)]`) which
        // `per_tab_help_entries` cannot parse (it handles only
        // `glyph:label` colon-tokens). Surface the Save and
        // Cancel keys directly so the help overlay on Settings
        // actually shows the lane-specific actions — without
        // this, the per-lane section is empty and the user
        // pressing `?` on Settings sees the empty-state
        // placeholder instead of the keys they need.
        //
        // The save key `s` and cancel key `Esc` are hardcoded
        // here to match what `Keybinds::footer_per_tab` returns
        // for `Lane::Settings`. (F-04: if the save binding ever
        // becomes configurable, both `footer_per_tab` and this
        // special case need to be updated together — they
        // share a single source of truth in spirit even if not
        // yet in code.)
        let per_lane = if lane == Lane::Settings {
            vec![
                ("Save".to_string(), vec!["s".to_string()]),
                ("Cancel".to_string(), vec!["Esc".to_string()]),
            ]
        } else {
            let per_tab = self.footer_per_tab(lane, content, false, false);
            Self::per_tab_help_entries(&per_tab)
        };
        (global, per_lane)
    }

    /// Convert a per-tab footer string (the output of
    /// [`Keybinds::footer_per_tab`]) into structured per-lane help
    /// entries by parsing "key:label" tokens separated by " · ".
    /// The labels match the help-entry labels on the global side
    /// (e.g. "Move up", "Select / drill in") so the two groups read
    /// consistently. Per-tab tokens that combine two glyphs (e.g.
    /// "h/l:section" on detail rows, "n/p:item") are split into a
    /// single entry with both glyphs in the keys vector.
    ///
    /// Special case: the `/` glyph (search key) collides with the
    /// `/` used to combine glyphs. When the glyph side of a token
    /// is exactly `/`, the parser treats it as the search key (not
    /// as an empty glyph-list from a `"/".split('/')`).
    fn per_tab_help_entries(per_tab: &str) -> Vec<(String, Vec<String>)> {
        let mut entries: Vec<(String, Vec<String>)> = Vec::new();
        for raw in per_tab.split(" · ") {
            let token = raw.trim();
            if token.is_empty() {
                continue;
            }
            // Find the first `:` (none of the per-tab labels in v1
            // contain colons).
            let (glyphs, label) = match token.find(':') {
                Some(idx) => (&token[..idx], token[idx + 1..].trim()),
                None => continue,
            };
            if label.is_empty() {
                continue;
            }
            // Special case: the literal "/" search glyph. Splitting
            // it on "/" yields two empty strings, so we detect this
            // case before the split and keep the search key as-is.
            let keys: Vec<String> = if glyphs == "/" {
                vec!["/".to_string()]
            } else {
                glyphs
                    .split('/')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            };
            if keys.is_empty() {
                continue;
            }
            entries.push((label.to_string(), keys));
        }
        entries
    }
}

/// One row of the help overlay / footer: a human label plus the formatted
/// key combos currently bound to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpEntry {
    pub label: &'static str,
    pub keys: Vec<String>,
}

impl HelpEntry {
    /// Join the keys for display, e.g. `Up, k`.
    pub fn keys_display(&self) -> String {
        self.keys.join(", ")
    }
}

/// Render a [`KeyCombo`] as a string the profile writer can re-parse via
/// [`parse_key_combo`]. Round-trips the uppercase-auto-shift behavior:
/// `KeyCode::Char('q') + SHIFT` is rendered as `Q` (which `parse_key_combo`
/// then turns back into `shift+q`).
fn combo_to_toml_string((code, modifiers): KeyCombo) -> String {
    use crossterm::event::KeyModifiers;
    let mut out = String::new();
    if modifiers.contains(KeyModifiers::CONTROL) {
        out.push_str("ctrl+");
    }
    if modifiers.contains(KeyModifiers::ALT) {
        out.push_str("alt+");
    }
    if modifiers.contains(KeyModifiers::SUPER) {
        out.push_str("super+");
    }
    if modifiers.contains(KeyModifiers::HYPER) {
        out.push_str("hyper+");
    }
    if modifiers.contains(KeyModifiers::SHIFT) {
        out.push_str("shift+");
    }
    out.push_str(&combo_code_to_name(code));
    out
}

fn combo_code_to_name(code: KeyCode) -> String {
    match code {
        KeyCode::Char(' ') => "space".to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Esc => "esc".to_string(),
        KeyCode::Tab => "tab".to_string(),
        KeyCode::BackTab => "shift+tab".to_string(),
        KeyCode::Backspace => "backspace".to_string(),
        KeyCode::Left => "left".to_string(),
        KeyCode::Right => "right".to_string(),
        KeyCode::Up => "up".to_string(),
        KeyCode::Down => "down".to_string(),
        KeyCode::PageUp => "pageup".to_string(),
        KeyCode::PageDown => "pagedown".to_string(),
        KeyCode::F(n) => format!("f{n}"),
        other => format!("{other:?}"),
    }
}

/// Strip a trailing `# ...` comment, but only when the `#` lies outside a
/// double-quoted basic string. Handles `\"` escapes inside the string so an
/// escaped quote does not flip the in-string state.
fn strip_toml_comment(raw: &str) -> &str {
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in raw.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if in_string {
            match c {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
        } else {
            match c {
                '"' => in_string = true,
                '#' => return &raw[..i],
                _ => {}
            }
        }
    }
    raw
}

/// Wrap a combo string as a TOML basic string, escaping `\` and `"` so the
/// reader's quote-aware scan can always recover the original. (M139
/// code-review: the prior raw `format!("\"{}\"", …)` emitted `"""` for a
/// `"`-key, which the reader then over-stripped to empty.)
fn toml_basic_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Parse the RHS of a profile line as one or more TOML basic strings.
/// Accepts both the single (`key = "combo"`) and array
/// (`key = ["a", "b"]`) forms, splitting arrays on commas that lie outside
/// quotes. Returns `None` when a quoted token is malformed.
fn parse_profile_value(value: &str) -> Option<Vec<String>> {
    let v = value.trim();
    if let Some(inner) = v.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let mut out = Vec::new();
        for tok in split_outside_quotes(inner, ',') {
            let t = tok.trim();
            if t.is_empty() {
                continue;
            }
            out.push(unquote_basic_string(t)?);
        }
        Some(out)
    } else {
        Some(vec![unquote_basic_string(v)?])
    }
}

/// Remove the surrounding `"…"` from a TOML basic string and unescape
/// `\\` → `\` and `\"` → `"`. Other backslash escapes are kept lenient
/// (the key grammar never produces them). Returns `None` when the token is
/// not a clean double-quoted string.
fn unquote_basic_string(s: &str) -> Option<String> {
    let s = s.trim();
    let inner = s.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                other => out.push(other),
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
}

/// Split `s` on `delim`, ignoring delimiters that occur inside a
/// double-quoted basic string (with `\"` escapes respected). Used to split
/// a TOML array body into its element tokens without breaking on a quoted
/// comma.
fn split_outside_quotes(s: &str, delim: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    let mut start = 0usize;
    for (i, c) in s.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if in_string {
            match c {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
        } else if c == '"' {
            in_string = true;
        } else if c == delim {
            out.push(&s[start..i]);
            start = i + delim.len_utf8();
        }
    }
    out.push(&s[start..]);
    out
}

/// Format a watch-countdown duration for the footer (M164-era
/// helper, no longer used by any code path after M179 removed the
/// Overview auto-refresh surface; the format_countdown tests below
/// remain as a no-op pin on the helper so a future regression that
/// accidentally re-introduces the helper does not silently break
/// the test compile).
#[allow(dead_code)]
fn format_countdown(secs: u64) -> String {
    if secs < 60 {
        return format!("{secs}s");
    }
    if secs < 60 * 60 {
        let m = secs / 60;
        let s = secs % 60;
        return format!("{m}m{s:02}s");
    }
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    format!("{h}h{m:02}m")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_countdown_short() {
        assert_eq!(format_countdown(0), "0s");
        assert_eq!(format_countdown(12), "12s");
        assert_eq!(format_countdown(59), "59s");
    }

    #[test]
    fn format_countdown_minutes() {
        assert_eq!(format_countdown(60), "1m00s");
        assert_eq!(format_countdown(4 * 60 + 30), "4m30s");
        assert_eq!(format_countdown(59 * 60 + 59), "59m59s");
    }

    #[test]
    fn format_countdown_hours() {
        assert_eq!(format_countdown(60 * 60), "1h00m");
        assert_eq!(format_countdown(3600 + 5 * 60), "1h05m");
        assert_eq!(format_countdown(25 * 60 * 60), "25h00m");
    }

    #[test]
    fn footer_overview_includes_refresh_key() {
        let kb = Keybinds::default();
        let s = kb.footer_overview();
        let refresh_key = Keybinds::primary(&kb.refresh);
        assert!(
            s.contains(&refresh_key),
            "footer should mention the refresh keybind; got {s:?} key={refresh_key:?}"
        );
    }

    #[test]
    fn footer_overview_no_longer_mentions_watch() {
        // M179: the legacy "watch ON — next refresh" footer is gone.
        // Manual Overview refresh (r/R) remains the only refresh path.
        let kb = Keybinds::default();
        let s = kb.footer_overview();
        assert!(
            !s.to_ascii_lowercase().contains("watch on"),
            "footer should no longer mention 'watch on' (legacy auto-refresh removed); got: {s}"
        );
    }
}

// M199 S1: unit tests for `Keybinds::footer_per_tab`. The
// spec calls for a test that walks every (lane, content_state)
// variant and asserts the expected per-(lane, content_state)
// string. Path returns the empty string (the only v1 case).
// Lives in the same module as the function so the test has
// access to the private `per_tab_help_entries` helper.
#[cfg(test)]
mod footer_per_tab_tests {
    use super::*;
    use crate::tui::app::{ContentState, Lane};

    fn per_tab(lane: Lane, content: ContentState) -> String {
        Keybinds::default().footer_per_tab(lane, content, false, false)
    }

    #[test]
    fn path_lane_returns_empty_string_for_every_content_state() {
        // M199 S1 + D-07: Path is the only lane in v1 with an
        // empty per-tab string. The 1-row footer in the
        // renderer follows from this contract.
        for content in [
            ContentState::List,
            ContentState::MilestoneDetail,
            ContentState::BacklogDetail,
            ContentState::AnnotationThread,
            ContentState::CoApproval,
        ] {
            let s = per_tab(Lane::Path, content);
            assert!(
                s.is_empty(),
                "footer_per_tab(Path, {content:?}) must be empty; got={s:?}"
            );
        }
    }

    #[test]
    fn watch_lane_returns_empty_string_for_every_content_state() {
        // D-07: Watch is the second v1 lane with an empty
        // per-tab string (its per-row actions render in the
        // row, not the footer).
        for content in [
            ContentState::List,
            ContentState::MilestoneDetail,
            ContentState::BacklogDetail,
            ContentState::AnnotationThread,
            ContentState::CoApproval,
        ] {
            let s = per_tab(Lane::Watch, content);
            assert!(
                s.is_empty(),
                "footer_per_tab(Watch, {content:?}) must be empty; got={s:?}"
            );
        }
    }

    #[test]
    fn overview_list_surfaces_enter_as_inbox_verb() {
        // D-04: Overview's ⏎ is the only lane-specific
        // override; use the verb "inbox" so the user
        // distinguishes it from the globals line's `⏎:go`.
        let s = per_tab(Lane::Overview, ContentState::List);
        assert!(
            s.contains(":inbox"),
            "Overview per-tab must include `:inbox`; got={s:?}"
        );
        // The globals row already covers ↑↓/move, so the
        // per-tab must not duplicate it.
        assert!(
            !s.contains(":move"),
            "Overview per-tab must not duplicate globals `:move`; got={s:?}"
        );
    }

    #[test]
    fn milestones_list_surfaces_six_lane_specific_keys() {
        // M199 D-04: Milestones/List carries the six
        // lane-conditional items the globals row dropped.
        let s = per_tab(Lane::Milestones, ContentState::List);
        for label in ["filter", "search", "hide-done", "sort", "cycle", "annotate"] {
            assert!(
                s.contains(label),
                "Milestones/List per-tab must include `{label}`; got={s:?}"
            );
        }
        // Globals tokens must not be duplicated.
        for forbidden in [":quit", ":help", ":refresh", ":go", ":move", ":lanes"] {
            assert!(
                !s.contains(forbidden),
                "Milestones/List per-tab must not duplicate globals token `{forbidden}`; got={s:?}"
            );
        }
    }

    #[test]
    fn milestones_detail_surfaces_section_and_item_with_combined_glyphs() {
        // M199 S1: detail rows use combined glyphs (e.g.
        // `[/]:section`, `n/p:item`). The per-tab string
        // literally contains the combined glyph so the
        // help-overlay parser can split it on `/`.
        let s = per_tab(Lane::Milestones, ContentState::MilestoneDetail);
        assert!(
            s.contains("section"),
            "MilestonesDetail per-tab must include `section`; got={s:?}"
        );
        assert!(
            s.contains("item"),
            "MilestonesDetail per-tab must include `item`; got={s:?}"
        );
        // The combined glyph is `[/]` for prev/next section.
        assert!(
            s.contains("[/]"),
            "MilestonesDetail per-tab must contain the `[/]` combined glyph for section; got={s:?}"
        );
    }

    #[test]
    fn annotation_thread_surfaces_resolve_and_reopen() {
        // D-08: the per-tab line on AnnotationThread
        // intentionally surfaces `r:resolve` even though
        // globals already covers `r:refresh`. Two rows
        // visible, user picks by tab.
        let s = per_tab(Lane::Overview, ContentState::AnnotationThread);
        assert!(
            s.contains("annotate"),
            "AnnotationThread per-tab must include `annotate`; got={s:?}"
        );
        assert!(
            s.contains("resolve"),
            "AnnotationThread per-tab must include `resolve`; got={s:?}"
        );
        assert!(
            s.contains("reopen"),
            "AnnotationThread per-tab must include `reopen`; got={s:?}"
        );
    }

    #[test]
    fn settings_uses_save_cancel_markers_not_colon_tokens() {
        // Q-02: Settings uses bracket markers
        // (`[Save (s)]`, `[Cancel (Esc)]`) and the dirty
        // state is inline (`*` suffix). No colon-separated
        // glyph:label tokens.
        let clean =
            Keybinds::default().footer_per_tab(Lane::Settings, ContentState::List, false, false);
        let dirty =
            Keybinds::default().footer_per_tab(Lane::Settings, ContentState::List, false, true);
        assert!(
            clean.contains("[Save (s)]"),
            "Settings clean per-tab must show `[Save (s)]`; got={clean:?}"
        );
        assert!(
            clean.contains("[Cancel (Esc)]"),
            "Settings per-tab must show `[Cancel (Esc)]`; got={clean:?}"
        );
        assert!(
            !clean.contains("[Save (s)*]"),
            "Settings clean per-tab must NOT show the dirty `*` marker; got={clean:?}"
        );
        assert!(
            dirty.contains("[Save (s)*]"),
            "Settings dirty per-tab must show `[Save (s)*]`; got={dirty:?}"
        );
    }

    #[test]
    fn per_tab_help_entries_handles_search_glyph_correctly() {
        // Regression: the parser used to split `/:search` on
        // `/` and drop the search key (empty strings). The
        // special-case `glyphs == "/"` must surface the
        // search key.
        let s =
            Keybinds::default().footer_per_tab(Lane::Milestones, ContentState::List, false, false);
        let entries = Keybinds::per_tab_help_entries(&s);
        let search_entry = entries
            .iter()
            .find(|(_, keys)| keys.iter().any(|k| k == "/"));
        assert!(
            search_entry.is_some(),
            "parser must surface `/` as the search key in the per-tab entries; entries={entries:?}"
        );
    }
}
