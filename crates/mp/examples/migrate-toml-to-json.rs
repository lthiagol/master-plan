//! One-shot driver for the M92 TOML → JSON plan migration.
//!
//! Run with: `cargo run -p mp --example migrate-toml-to-json -- <plan-dir>...`
//!
//! Each argument is a plan directory whose `*.toml` artifacts are converted to
//! `*.json` in place (originals removed). Re-runs are idempotent: a tree with
//! no `.toml` files converts nothing.
//!
//! This was used once to migrate the dogfood `master-plan/` and every
//! `tests/fixtures/projects/*` fixture; it is retained as the record of how
//! the on-disk conversion was performed.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: migrate-toml-to-json <plan-dir>...");
        return ExitCode::from(2);
    }
    let mut total_converted = 0usize;
    let mut errors = 0usize;
    for arg in &args {
        let plan_dir = PathBuf::from(arg);
        match mp::migrate::migrate_plan_dir(&plan_dir) {
            Ok(report) => {
                for c in &report.converted {
                    println!("  {} -> {}", c.from.display(), c.to.display());
                }
                println!(
                    "{}: {} converted, {} skipped",
                    plan_dir.display(),
                    report.converted.len(),
                    report.skipped.len()
                );
                total_converted += report.converted.len();
            }
            Err(e) => {
                eprintln!("{}: {e:#}", plan_dir.display());
                errors += 1;
            }
        }
    }
    println!("total converted: {total_converted}");
    if errors > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
