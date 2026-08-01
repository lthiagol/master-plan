use clap::error::ErrorKind;
use clap::Parser;
use mp::cli::Cli;

fn main() -> std::process::ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let cli = match Cli::try_parse_from(&args) {
        Ok(cli) => cli,
        Err(e) => {
            // `DisplayHelp` / `DisplayVersion` are friendly exits — exit 0 without our hint.
            match e.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                    let _ = e.print();
                    return std::process::ExitCode::SUCCESS;
                }
                _ => {}
            }
            // Print clap's own diagnostic (includes "For more information, try '--help'")
            let _ = e.print();
            // Layer a one-line hint for the most common `mp milestone …` misroutes.
            // Design constraint: keep the hint LOWERCASE, single line, and point
            // at the correct command. See BF-03 in tracks/bugfix.json.
            if let Some(hint) = milestone_hint(&args) {
                eprintln!("hint: {hint}");
            }
            return std::process::ExitCode::from(2);
        }
    };
    match mp::app::run(cli) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            if let Some(code) = e.downcast_ref::<mp::ExitCode>() {
                return std::process::ExitCode::from(code.0 as u8);
            }
            eprintln!("Error: {e}");
            std::process::ExitCode::from(1)
        }
    }
}

/// One-line hint for the most common `mp milestone …` misroutes seen in agent sessions.
/// Conservative: only fires when the failing subcommand token (1) literally is `list`
/// or `show`, or (2) looks like a milestone / track-item id placed before the verb.
/// Returns None for any other case so we never spam noisy hints.
fn milestone_hint(args: &[std::ffi::OsString]) -> Option<&'static str> {
    let strs: Vec<&str> = args.iter().skip(1).filter_map(|a| a.to_str()).collect();
    if strs.first().copied() != Some("milestone") {
        return None;
    }
    let sub = strs.get(1).copied()?;
    match sub {
        "list" => Some(
            "try `mp list milestones` -- top-level lists live under `mp list <entity>` \
             (milestones, backlog, tracks, archived, decisions, ideas, ...)",
        ),
        "show" => {
            Some("try `mp show milestone <ID>` -- `mp show milestone --help` lists available flags")
        }
        _ => {
            // Heuristic: looks like an id placed before the verb (M91, M19.1, BF-03, TW-02, BG-12)
            let is_id_like = (sub.starts_with('M')
                && sub[1..].chars().next().is_some_and(|c| c.is_ascii_digit()))
                || sub.starts_with("BF-")
                || sub.starts_with("TW-")
                || sub.starts_with("BG-");
            if is_id_like {
                Some(
                    "the read path is `mp show milestone <ID>`; commands that mutate live under \
                     `mp milestone <verb> <ID>` -- the id comes AFTER the verb, not before",
                )
            } else {
                None
            }
        }
    }
}
