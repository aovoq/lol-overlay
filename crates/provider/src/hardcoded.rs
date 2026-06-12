//! Placeholder [`BuildProvider`]: enough real logic to drive the overlay end to
//! end, but the data is a tiny built-in table, not a live stats backend.
//!
//! It DOES demonstrate the core idea — recommendations react to the enemy team:
//! lots of AD on the enemy → it suggests armor; lots of AP → magic resist.
//! Replace this with a real provider when you wire up a data source.

use async_trait::async_trait;
use overlay_types::{GameSnapshot, ItemRecommendation, RuneRecommendation};

use crate::threat::classify_threats;
use crate::trait_def::BuildProvider;
use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DamageType {
    Physical,
    Magic,
    Tank,
    Unknown,
}

/// Minimal champion → damage-type lookup keyed by the locale-independent
/// English name (no spaces/apostrophes — matches `rawChampionName`, e.g.
/// "Chogath", "Kaisa", "MasterYi"). Placeholder for real champion data.
pub fn champion_damage_type(name: &str) -> DamageType {
    const PHYSICAL: &[&str] = &[
        "Zed", "Yasuo", "Yone", "Jhin", "Jinx", "Caitlyn", "Draven", "Tryndamere", "Talon",
        "MasterYi", "Vayne", "LeeSin", "Renekton", "Riven", "Camille", "Khazix", "Graves",
        "Kaisa", "Xayah", "Kayn", "Kindred", "Lucian",
    ];
    const MAGIC: &[&str] = &[
        "Ahri", "Syndra", "Lux", "Veigar", "Brand", "Karthus", "Viktor", "Annie", "Orianna",
        "Cassiopeia", "Morgana", "TwistedFate", "Ryze", "Vladimir", "Diana", "Malzahar", "Azir",
        "Xerath", "Vex",
    ];
    const TANK: &[&str] = &[
        "Malphite", "Ornn", "Sion", "Chogath", "Sejuani", "Zac", "Leona", "Nautilus", "Maokai",
        "Shen", "Rammus", "DrMundo", "Udyr", "Poppy", "Zac", "Amumu", "Braum",
    ];
    if PHYSICAL.contains(&name) {
        DamageType::Physical
    } else if MAGIC.contains(&name) {
        DamageType::Magic
    } else if TANK.contains(&name) {
        DamageType::Tank
    } else {
        DamageType::Unknown
    }
}

/// Offline fallback provider. Not wired in by default (DeepLoL is), but kept as
/// a zero-dependency backend you can drop into `lib.rs` when the network is out.
#[allow(dead_code)]
pub struct HardcodedProvider;

#[async_trait]
impl BuildProvider for HardcodedProvider {
    async fn items(&self, snapshot: &GameSnapshot) -> Result<Vec<ItemRecommendation>> {
        let threats = classify_threats(snapshot);
        let mut recs: Vec<ItemRecommendation> = Vec::new();

        // Defensive item keyed to the dominant enemy damage type.
        if threats.ad_count >= threats.ap_count && threats.ad_count > 0 {
            recs.push(ItemRecommendation {
                item_id: 3047,
                name: "Plated Steelcaps".into(),
                score: 0.9,
                reason: format!("{} AD threats on enemy team", threats.ad_count),
            });
            if threats.ad_count >= 3 {
                recs.push(ItemRecommendation {
                    item_id: 3143,
                    name: "Randuin's Omen".into(),
                    score: 0.85,
                    reason: "Heavy AD / crit comp".into(),
                });
            }
        } else if threats.ap_count > 0 {
            recs.push(ItemRecommendation {
                item_id: 3111,
                name: "Mercury's Treads".into(),
                score: 0.9,
                reason: format!("{} AP threats on enemy team", threats.ap_count),
            });
            if threats.ap_count >= 3 {
                recs.push(ItemRecommendation {
                    item_id: 3065,
                    name: "Spirit Visage".into(),
                    score: 0.85,
                    reason: "Heavy AP comp".into(),
                });
            }
        }

        if threats.tank_count >= 2 {
            recs.push(ItemRecommendation {
                item_id: 3036,
                name: "Lord Dominik's Regards".into(),
                score: 0.8,
                reason: format!("{} tanks to shred", threats.tank_count),
            });
        }

        // A generic core item so there's always something to show.
        recs.push(ItemRecommendation {
            item_id: 6692,
            name: "Eclipse".into(),
            score: 0.5,
            reason: "Generic core (placeholder data)".into(),
        });

        Ok(recs)
    }

    async fn runes(&self, _champion_id: i64, _role: Option<&str>) -> Result<RuneRecommendation> {
        // A single generic Conqueror page for everyone. Real provider would key
        // off (champion_id, role) and return the highest-winrate page.
        Ok(RuneRecommendation {
            name: "Auto: Conqueror".into(),
            primary_style_id: 8000, // Precision
            sub_style_id: 8100,     // Domination
            selected_perk_ids: vec![
                8010, // Conqueror
                9111, // Triumph
                9104, // Legend: Alacrity
                8299, // Last Stand
                8143, // Sudden Impact
                8135, // Treasure Hunter
                5008, // Adaptive Force
                5008, // Adaptive Force
                5001, // Health
            ],
        })
    }
}
