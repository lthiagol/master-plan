use clap::ValueEnum;

/// M102: lane selector for `mp path --lane <name>`.
///
/// M102 R3: each variant carries its source-of-truth string. The
/// `as_str()` method is what `cmd_status` / `cmd_next_step` use to
/// look up the corresponding lane in `LaneReport::lanes` by name
/// (instead of positional index, which silently drifts on enum
/// reorders / renames / conditional lane emission).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LaneArg {
    Blocked,
    Execution,
    Review,
    Grooming,
    Backlog,
}

impl LaneArg {
    pub fn as_str(&self) -> &'static str {
        match self {
            LaneArg::Blocked => "blocked",
            LaneArg::Execution => "execution",
            LaneArg::Review => "review",
            LaneArg::Grooming => "grooming",
            LaneArg::Backlog => "backlog",
        }
    }
}
