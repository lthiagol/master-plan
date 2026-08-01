use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    Bugfix,
    Tweak,
}

impl TrackKind {
    pub const ALL: [TrackKind; 2] = [TrackKind::Bugfix, TrackKind::Tweak];

    pub fn as_str(&self) -> &'static str {
        match self {
            TrackKind::Bugfix => "bugfix",
            TrackKind::Tweak => "tweak",
        }
    }

    pub fn prefix(&self) -> &'static str {
        match self {
            TrackKind::Bugfix => "BF",
            TrackKind::Tweak => "TW",
        }
    }
}

impl std::str::FromStr for TrackKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "bugfix" => Ok(TrackKind::Bugfix),
            "tweak" => Ok(TrackKind::Tweak),
            _ => Err(format!("invalid track kind: {s}")),
        }
    }
}

impl std::fmt::Display for TrackKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
