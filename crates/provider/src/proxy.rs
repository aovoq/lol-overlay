//! Provider identifiers shared by build and player-stat registries.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    #[default]
    Deeplol,
    Ugg,
    Lolalytics,
    Lolps,
    Opgg,
}

impl<'de> Deserialize<'de> for ProviderKind {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self::parse(&s).unwrap_or_default())
    }
}

impl ProviderKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "deeplol" => Some(Self::Deeplol),
            "ugg" => Some(Self::Ugg),
            "lolalytics" => Some(Self::Lolalytics),
            "lolps" => Some(Self::Lolps),
            "opgg" => Some(Self::Opgg),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Deeplol => "deeplol",
            Self::Ugg => "ugg",
            Self::Lolalytics => "lolalytics",
            Self::Lolps => "lolps",
            Self::Opgg => "opgg",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lolps_round_trips_through_parse_and_serde() {
        assert_eq!(ProviderKind::parse("lolps"), Some(ProviderKind::Lolps));
        assert_eq!(ProviderKind::Lolps.as_str(), "lolps");
        assert_eq!(
            serde_json::to_string(&ProviderKind::Lolps).unwrap(),
            "\"lolps\""
        );
        assert_eq!(
            serde_json::from_str::<ProviderKind>("\"lolps\"").unwrap(),
            ProviderKind::Lolps
        );
    }
}
