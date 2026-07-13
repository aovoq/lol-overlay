//! Provider identifiers shared by build and player-stat registries.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    #[default]
    Deeplol,
    Ugg,
    Lolalytics,
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
            "opgg" => Some(Self::Opgg),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Deeplol => "deeplol",
            Self::Ugg => "ugg",
            Self::Lolalytics => "lolalytics",
            Self::Opgg => "opgg",
        }
    }
}
