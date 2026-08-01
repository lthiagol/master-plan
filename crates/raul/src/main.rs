use clap::error::{ContextKind, ContextValue, ErrorKind};
use clap::Parser;

use anyhow::Result;

use raul::cli::{Cli, M164_SENTINEL};
use raul::mp_runner::MpRunner;

fn main() -> Result<()> {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            // The M164 sentinel is reserved for users with stale muscle
            // memory (`raul status` etc.). clap 4 collapses
            // "UnrecognizedSubcommand" and "UnknownArgument" into the same
            // `ErrorKind::UnknownArgument`, so we distinguish by inspecting
            // the offending arg via `ContextKind::InvalidArg`:
            //   * a flag-shaped error starts with `--` / `-` → clap's
            //     usage hint is the right answer
            //   * a bare subcommand attempt has no leading `-` → the
            //     migration message is more useful than "unknown argument
            //     'status'"
            match e.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                    let _ = e.print();
                    return Ok(());
                }
                ErrorKind::UnknownArgument if is_unknown_subcommand(&e) => {
                    eprintln!("{M164_SENTINEL}");
                    std::process::exit(2);
                }
                _ => {
                    let _ = e.print();
                    std::process::exit(2);
                }
            }
        }
    };

    let mut runner = MpRunner::new()?;
    if let Some(root) = cli.project_root {
        runner.set_project_root(root);
    }
    if let Some(dir) = cli.plan_dir {
        runner.set_plan_dir(dir);
    }

    let ui = raul::config::UiConfig::load(&runner).with_color_override(cli.color);
    raul::config::set_color_enabled(ui.color);
    raul::config::set_icons(ui.icons);

    raul::tui::runner::run_tui(&runner, raul::tui::runner::TuiOptions::default())
}

/// True when the offending `UnknownArgument` is a bare token (subcommand
/// attempt) rather than a flag. clap 4 stores the arg in
/// `ContextKind::InvalidArg`; we treat anything starting with `-` as a
/// flag and emit clap's usage hint instead of the migration sentinel.
fn is_unknown_subcommand(e: &clap::error::Error) -> bool {
    let Some(ContextValue::String(arg)) = e.get(ContextKind::InvalidArg) else {
        // No `InvalidArg` context — be conservative and call it a flag
        // error so the user always sees clap's hint.
        return false;
    };
    !arg.starts_with('-')
}
