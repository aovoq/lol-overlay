// Credit to https://github.com/pradishb/ugg-parser for figuring out the
// structure of the champ overview stats data.

use serde::de::{Deserialize, Deserializer, IgnoredAny, SeqAccess, Visitor};
use serde::Serialize;
use std::fmt;

use super::overview::handle_unknown;

#[derive(Debug, Clone, Serialize)]
pub struct OverviewData {
    pub runes: Runes,
    pub summoner_spells: SummonerSpells,
    pub starting_items: Items,
    pub core_items: Items,
    pub abilities: Abilities,
    pub item_4_options: Vec<LateItem>,
    pub item_5_options: Vec<LateItem>,
    pub item_6_options: Vec<LateItem>,
    pub wins: i64,
    pub matches: i64,
    pub low_sample_size: bool,
    pub shards: Shards,
}

#[derive(Debug, Clone, Serialize)]
pub struct Runes {
    pub matches: i64,
    pub wins: i64,
    pub primary_style_id: i64,
    pub secondary_style_id: i64,
    pub rune_ids: Vec<i64>,
}

impl<'de> Deserialize<'de> for Runes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RunesVisitor;

        impl<'de> Visitor<'de> for RunesVisitor {
            type Value = Runes;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("rune stats")
            }

            fn visit_seq<V>(self, mut visitor: V) -> Result<Runes, V::Error>
            where
                V: SeqAccess<'de>,
            {
                Ok(Runes {
                    matches: handle_unknown(visitor.next_element::<i64>()),
                    wins: handle_unknown(visitor.next_element::<i64>()),
                    primary_style_id: handle_unknown(visitor.next_element::<i64>()),
                    secondary_style_id: handle_unknown(visitor.next_element::<i64>()),
                    rune_ids: handle_unknown(visitor.next_element::<Vec<i64>>()),
                })
            }
        }

        deserializer.deserialize_seq(RunesVisitor)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SummonerSpells {
    pub matches: i64,
    pub wins: i64,
    pub spell_ids: Vec<i64>,
}

impl<'de> Deserialize<'de> for SummonerSpells {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SummonerSpellsVisitor;

        impl<'de> Visitor<'de> for SummonerSpellsVisitor {
            type Value = SummonerSpells;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("summoner spells")
            }

            fn visit_seq<V>(self, mut visitor: V) -> Result<SummonerSpells, V::Error>
            where
                V: SeqAccess<'de>,
            {
                Ok(SummonerSpells {
                    matches: handle_unknown(visitor.next_element::<i64>()),
                    wins: handle_unknown(visitor.next_element::<i64>()),
                    spell_ids: handle_unknown(visitor.next_element::<Vec<i64>>()),
                })
            }
        }

        deserializer.deserialize_seq(SummonerSpellsVisitor)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Items {
    pub matches: i64,
    pub wins: i64,
    pub item_ids: Vec<i64>,
}

impl<'de> Deserialize<'de> for Items {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ItemsVisitor;

        impl<'de> Visitor<'de> for ItemsVisitor {
            type Value = Items;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("items")
            }

            fn visit_seq<V>(self, mut visitor: V) -> Result<Items, V::Error>
            where
                V: SeqAccess<'de>,
            {
                Ok(Items {
                    matches: handle_unknown(visitor.next_element::<i64>()),
                    wins: handle_unknown(visitor.next_element::<i64>()),
                    item_ids: handle_unknown(visitor.next_element::<Vec<i64>>()),
                })
            }
        }

        deserializer.deserialize_seq(ItemsVisitor)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Abilities {
    pub matches: i64,
    pub wins: i64,
    pub ability_order: Vec<char>,
    pub ability_max_order: String,
}

impl<'de> Deserialize<'de> for Abilities {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct AbilitiesVisitor;

        impl<'de> Visitor<'de> for AbilitiesVisitor {
            type Value = Abilities;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("abilities")
            }

            fn visit_seq<V>(self, mut visitor: V) -> Result<Abilities, V::Error>
            where
                V: SeqAccess<'de>,
            {
                Ok(Abilities {
                    matches: handle_unknown(visitor.next_element::<i64>()),
                    wins: handle_unknown(visitor.next_element::<i64>()),
                    ability_order: handle_unknown(visitor.next_element::<Vec<char>>()),
                    ability_max_order: handle_unknown(visitor.next_element::<String>()),
                })
            }
        }

        deserializer.deserialize_seq(AbilitiesVisitor)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LateItem {
    pub matches: i64,
    pub wins: i64,
    pub id: i64,
}

impl<'de> Deserialize<'de> for LateItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LateItemVisitor;

        impl<'de> Visitor<'de> for LateItemVisitor {
            type Value = LateItem;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("summoner spells")
            }

            fn visit_seq<V>(self, mut visitor: V) -> Result<LateItem, V::Error>
            where
                V: SeqAccess<'de>,
            {
                Ok(LateItem {
                    id: handle_unknown(visitor.next_element::<i64>()),
                    wins: handle_unknown(visitor.next_element::<i64>()),
                    matches: handle_unknown(visitor.next_element::<i64>()),
                })
            }
        }

        deserializer.deserialize_seq(LateItemVisitor)
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Shards {
    pub matches: i64,
    pub wins: i64,
    pub shard_ids: Vec<i64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
enum ShardId {
    Number(i64),
    String(String),
}

impl ShardId {
    fn into_i64(self) -> Option<i64> {
        match self {
            ShardId::Number(id) => Some(id),
            ShardId::String(id) => id.parse().ok(),
        }
        .filter(|id| *id > 0)
    }
}

impl<'de> Deserialize<'de> for Shards {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct AbilitiesVisitor;

        impl<'de> Visitor<'de> for AbilitiesVisitor {
            type Value = Shards;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("shards")
            }

            fn visit_seq<V>(self, mut visitor: V) -> Result<Shards, V::Error>
            where
                V: SeqAccess<'de>,
            {
                Ok(Shards {
                    matches: handle_unknown(visitor.next_element::<i64>()),
                    wins: handle_unknown(visitor.next_element::<i64>()),
                    shard_ids: handle_unknown(visitor.next_element::<Vec<ShardId>>())
                        .into_iter()
                        .filter_map(ShardId::into_i64)
                        .collect::<Vec<i64>>(),
                })
            }
        }

        deserializer.deserialize_seq(AbilitiesVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::{OverviewData, Shards};
    use serde_json::json;

    #[test]
    fn shards_parse_string_ids() {
        let shards: Shards =
            serde_json::from_str(r#"[123, 45, ["5005", "5008", "5001"]]"#).expect("shards");

        assert_eq!(shards.matches, 123);
        assert_eq!(shards.wins, 45);
        assert_eq!(shards.shard_ids, vec![5005, 5008, 5001]);
    }

    #[test]
    fn shards_parse_numeric_ids() {
        let shards: Shards =
            serde_json::from_str(r#"[123, 45, [5005, 5008, 5001]]"#).expect("shards");

        assert_eq!(shards.matches, 123);
        assert_eq!(shards.wins, 45);
        assert_eq!(shards.shard_ids, vec![5005, 5008, 5001]);
    }

    #[test]
    fn overview_rejects_truncated_late_items() {
        let overview = serde_json::from_value::<OverviewData>(json!([
            [10, 6, 8000, 8100, [8112, 8143]],
            [10, 6, [4, 14]],
            [10, 6, [1055, 2003]],
            [10, 6, [6672, 3006, 3031]],
            [10, 6, ["Q", "W", "E"], "QWE"],
            [[[3031, 6, 10]], [[3094, 5, 10]]],
            [6, 10],
            false,
            [10, 6, [5005, 5008, 5001]]
        ]));

        assert!(overview.is_err());
    }

    #[test]
    fn overview_rejects_truncated_match_info() {
        let overview = serde_json::from_value::<OverviewData>(json!([
            [10, 6, 8000, 8100, [8112, 8143]],
            [10, 6, [4, 14]],
            [10, 6, [1055, 2003]],
            [10, 6, [6672, 3006, 3031]],
            [10, 6, ["Q", "W", "E"], "QWE"],
            [[[3031, 6, 10]], [[3094, 5, 10]], [[3072, 4, 10]]],
            [6],
            false,
            [10, 6, [5005, 5008, 5001]]
        ]));

        assert!(overview.is_err());
    }
}

impl<'de> Deserialize<'de> for OverviewData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct OverviewDataVisitor;

        impl<'de> Visitor<'de> for OverviewDataVisitor {
            type Value = OverviewData;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("overview data")
            }

            fn visit_seq<V>(self, mut visitor: V) -> Result<OverviewData, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let runes = visitor
                    .next_element::<Runes>()?
                    .ok_or(serde::de::Error::custom("Could not parse runes."))?;
                let summoner_spells = visitor
                    .next_element::<SummonerSpells>()?
                    .ok_or(serde::de::Error::custom("Could not parse summoner spells."))?;
                let starting_items = visitor
                    .next_element::<Items>()?
                    .ok_or(serde::de::Error::custom("Could not parse starting items."))?;
                let core_items = visitor
                    .next_element::<Items>()?
                    .ok_or(serde::de::Error::custom("Could not parse core items."))?;
                let abilities = visitor
                    .next_element::<Abilities>()?
                    .ok_or(serde::de::Error::custom("Could not parse abilities."))?;
                let late_items = visitor
                    .next_element::<Vec<Vec<LateItem>>>()?
                    .ok_or(serde::de::Error::custom("Could not parse late items."))?;
                let match_info = visitor
                    .next_element::<Vec<i64>>()?
                    .ok_or_else(|| serde::de::Error::custom("Could not parse match info."))?;
                let wins = match_info
                    .first()
                    .copied()
                    .ok_or_else(|| serde::de::Error::custom("Could not parse wins."))?;
                let matches = match_info
                    .get(1)
                    .copied()
                    .ok_or_else(|| serde::de::Error::custom("Could not parse matches."))?;
                let low_sample_size = matches < 1000;

                // this is the original low sample size value, it's always false though, so ignore.
                let _ = visitor.next_element::<IgnoredAny>().is_ok();

                let shards = visitor
                    .next_element::<Shards>()
                    .unwrap_or_default()
                    .unwrap_or_default();

                // Don't know what this is yet
                while let Some(IgnoredAny) = visitor.next_element()? {}

                let item_4_options = late_items
                    .first()
                    .cloned()
                    .ok_or_else(|| serde::de::Error::custom("Could not parse item 4 options."))?;
                let item_5_options = late_items
                    .get(1)
                    .cloned()
                    .ok_or_else(|| serde::de::Error::custom("Could not parse item 5 options."))?;
                let item_6_options = late_items
                    .get(2)
                    .cloned()
                    .ok_or_else(|| serde::de::Error::custom("Could not parse item 6 options."))?;

                let overview_data = OverviewData {
                    runes,
                    summoner_spells,
                    starting_items,
                    core_items,
                    abilities,
                    item_4_options,
                    item_5_options,
                    item_6_options,
                    wins,
                    matches,
                    low_sample_size,
                    shards,
                };
                Ok(overview_data)
            }
        }

        deserializer.deserialize_seq(OverviewDataVisitor)
    }
}
