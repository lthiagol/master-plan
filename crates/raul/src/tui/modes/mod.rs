//! M136: per-mode handler modules. One file per `Mode` variant.
//!
//! Each module exposes a single `handle_key(key, app) -> Vec<Action>` (or
//! the variant-specific signature documented inline). Handlers are pure —
//! no `App` mutation, no `MpRunner` shell-out, no I/O. The dispatcher in
//! [`super::runner`] folds the returned `Vec<Action>` through
//! [`crate::tui::action::apply_action`], which is the single place that
//! mutates `App` and shells out to `mp`.
//!
//! Per-handler file layout:
//!
//!   * `normal.rs`             — `Mode::Normal` (default mode).
//!   * `input.rs`              — `Mode::Input(_)` text input overlay.
//!   * `help.rs`               — `Mode::Help`.
//!   * `annotation_thread.rs`  — `Mode::AnnotationThread`.
//!   * `review_menu.rs`        — `Mode::ReviewMenu(_)`.
//!   * `settings.rs`           — Settings lane key handler (M169).

pub mod annotation_thread;
pub mod filter_modal;
pub mod help;
pub mod input;
pub mod lifecycle_filter;
pub mod normal;
pub mod review_menu;
pub mod search_input;
pub mod settings;
