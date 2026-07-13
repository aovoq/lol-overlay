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

#[cfg(test)]
mod tests {
    use super::*;
    use overlay_types::EnemyChampion;

    #[test]
    fn classifies_enemy_damage_mix() {
        let snapshot = GameSnapshot {
            game_mode: "CLASSIC".into(),
            game_time: 0.0,
            self_champion: "Ahri".into(),
            self_raw_name: "Ahri".into(),
            self_position: "middle".into(),
            allies: Vec::new(),
            players: vec![],
            enemies: vec![
                EnemyChampion {
                    name: "Zed".into(),
                    raw_name: "Zed".into(),
                    position: "middle".into(),
                    items: Vec::new(),
                },
                EnemyChampion {
                    name: "Lux".into(),
                    raw_name: "Lux".into(),
                    position: "middle".into(),
                    items: Vec::new(),
                },
                EnemyChampion {
                    name: "Malphite".into(),
                    raw_name: "Malphite".into(),
                    position: "top".into(),
                    items: Vec::new(),
                },
                EnemyChampion {
                    name: "Leona".into(),
                    raw_name: "Leona".into(),
                    position: "utility".into(),
                    items: Vec::new(),
                },
            ],
        };

        let threats = classify_threats(&snapshot);
        assert_eq!(threats.ad_count, 1);
        assert_eq!(threats.ap_count, 1);
        assert_eq!(threats.tank_count, 2);
        assert!(threats.cc_heavy);
    }
}
