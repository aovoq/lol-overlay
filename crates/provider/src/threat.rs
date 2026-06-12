use overlay_types::{GameSnapshot, ThreatProfile};

use crate::hardcoded;

/// Cheap heuristic threat classifier shared by providers. This is a placeholder
/// — it keys off a tiny built-in champion list. The real version belongs in the
/// data layer with full champion damage data.
pub fn classify_threats(snapshot: &GameSnapshot) -> ThreatProfile {
    let mut p = ThreatProfile::default();
    for e in &snapshot.enemies {
        match hardcoded::champion_damage_type(&e.raw_name) {
            hardcoded::DamageType::Physical => p.ad_count += 1,
            hardcoded::DamageType::Magic => p.ap_count += 1,
            hardcoded::DamageType::Tank => p.tank_count += 1,
            hardcoded::DamageType::Unknown => {}
        }
    }
    p.cc_heavy = snapshot.enemies.len() >= 3 && p.tank_count >= 2;
    p
}
