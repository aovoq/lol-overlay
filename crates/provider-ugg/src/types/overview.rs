// Credit to https://github.com/pradishb/ugg-parser for figuring out the
// structure of the champ overview stats data.

use super::default_overview::OverviewData;
use super::mappings;
use serde::de::{Deserialize, Deserializer, IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize as DeserializeDerive, Serialize};
use std::collections::HashMap;
use std::fmt;

pub type ChampOverview = HashMap<
    mappings::Region,
    HashMap<mappings::Rank, HashMap<mappings::Role, WrappedOverviewData>>,
>;

pub fn handle_unknown<T: Default, E>(result: Result<Option<T>, E>) -> T {
    result.ok().flatten().unwrap_or_default()
}

#[derive(Debug, Clone, Serialize, DeserializeDerive)]
#[serde(untagged)]
pub enum Overview {
    Default(OverviewData),
}

impl Overview {
    #[must_use]
    pub fn matches(&self) -> i64 {
        match self {
            Overview::Default(d) => d.matches,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct WrappedOverviewData {
    pub data: Overview,
}

impl<'de> Deserialize<'de> for WrappedOverviewData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct WrappedOverviewDataVisitor;

        impl<'de> Visitor<'de> for WrappedOverviewDataVisitor {
            type Value = WrappedOverviewData;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("waa")
            }

            fn visit_seq<V>(self, mut visitor: V) -> Result<WrappedOverviewData, V::Error>
            where
                V: SeqAccess<'de>,
            {
                match visitor.next_element::<Overview>() {
                    Ok(Some(data)) => {
                        while let Some(IgnoredAny) = visitor.next_element()? {}
                        Ok(WrappedOverviewData { data })
                    }
                    Err(e) => Err(e),
                    _ => Err(serde::de::Error::custom("No more data left.")),
                }
            }
        }

        deserializer.deserialize_seq(WrappedOverviewDataVisitor)
    }
}
