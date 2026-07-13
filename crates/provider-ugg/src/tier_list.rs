//! u.gg `champion_ranking` stats2 payload → [`TierEntry`] rows.
//!
//! Endpoint shape (from u.gg tier-list SSR):
//! `…/champion_ranking/{region}/{patch}/{mode}/{rank_tier}/{version}.json`
//!
//! Top-level JSON is a 4-tuple: `[roles, _rank_scores, _updated_at, _scale]`.
//! Each role array holds variable-length rows; indices 2/3 are win
//! numerator/denominator.

use std::collections::HashMap;

use overlay_provider::ProviderError;
use overlay_types::TierEntry;
use serde_json::Value;

/// LCU / overlay role string → u.gg `champion_ranking` role key.
#[must_use]
pub fn tier_list_role_key(role: &str) -> Option<&'static str> {
    match role.to_ascii_lowercase().as_str() {
        "top" => Some("top"),
        "jungle" => Some("jungle"),
        "middle" | "mid" => Some("mid"),
        "bottom" | "bot" | "adc" => Some("adc"),
        "utility" | "support" | "supp" => Some("supp"),
        _ => None,
    }
}

/// Region slug in the stats2 path (`jp1`, `world`, …).
#[must_use]
pub fn region_slug(platform_id: &str) -> &'static str {
    match platform_id.to_ascii_lowercase().as_str() {
        "na1" => "na1",
        "euw1" => "euw1",
        "kr" => "kr",
        "eun1" => "eun1",
        "br1" => "br1",
        "la1" => "la1",
        "la2" => "la2",
        "oc1" => "oc1",
        "ru" => "ru",
        "tr1" => "tr1",
        "jp1" => "jp1",
        "ph2" => "ph2",
        "sg2" => "sg2",
        "th2" => "th2",
        "tw2" => "tw2",
        "vn2" => "vn2",
        "me1" => "me1",
        _ => "world",
    }
}

/// Default rank bracket on <https://u.gg/lol/tier-list> (Emerald+).
pub const TIER_LIST_RANK: &str = "emerald_plus";

/// Minimum sample (denominator) for a tier-list row; filters placeholder 1-game rows.
const MIN_TIER_GAMES: i64 = 30;

/// Keep the champ-select strip focused on champions that are actually played.
const MIN_TIER_PICK_RATE: f64 = 0.005;

struct RawRow {
    champion_id: i64,
    win_num: i64,
    win_den: i64,
    pick_weight: i64,
    ban_weight: i64,
}

fn parse_raw_row(row: &Value) -> Option<RawRow> {
    let arr = row.as_array()?;
    if arr.len() < 9 {
        return None;
    }
    let champion_id = arr[0].as_str()?.parse().ok()?;
    let win_num = arr[2].as_i64()?;
    let win_den = arr[3].as_i64()?;
    if win_den < MIN_TIER_GAMES || win_num > win_den {
        return None;
    }
    Some(RawRow {
        champion_id,
        win_num,
        win_den,
        pick_weight: arr[6].as_i64().unwrap_or(0),
        ban_weight: arr[8].as_i64().unwrap_or(0),
    })
}

fn role_rows<'a>(payload: &'a Value, role_key: &str) -> Result<&'a Vec<Value>, ProviderError> {
    let roles_obj = payload
        .get(0)
        .and_then(Value::as_object)
        .ok_or_else(|| ProviderError::Other("champion_ranking missing role map".into()))?;

    roles_obj
        .get(role_key)
        .and_then(Value::as_array)
        .ok_or_else(|| ProviderError::Other(format!("no {role_key} data in champion_ranking")))
}

#[allow(clippy::cast_precision_loss)]
fn previous_win_rates(payload: Option<&Value>, role_key: &str) -> HashMap<i64, f64> {
    payload
        .and_then(|p| role_rows(p, role_key).ok())
        .map(|rows| {
            rows.iter()
                .filter_map(parse_raw_row)
                .map(|row| (row.champion_id, row.win_num as f64 / row.win_den as f64))
                .collect()
        })
        .unwrap_or_default()
}

/// Build tier rows for one role out of a fetched `champion_ranking` blob.
#[allow(clippy::cast_precision_loss)]
pub fn tier_entries_from_ranking(
    payload: &Value,
    previous_payload: Option<&Value>,
    role: &str,
) -> Result<Vec<TierEntry>, ProviderError> {
    let role_key = tier_list_role_key(role)
        .ok_or_else(|| ProviderError::Other(format!("unknown role: {role:?}")))?;

    let rows = role_rows(payload, role_key)?;

    let mut raw: Vec<RawRow> = rows.iter().filter_map(parse_raw_row).collect();
    if raw.is_empty() {
        return Err(ProviderError::NotEnoughData);
    }

    let pick_sum: i64 = raw.iter().map(|r| r.pick_weight).sum();
    let ban_sum: i64 = raw.iter().map(|r| r.ban_weight).sum();
    let previous = previous_win_rates(previous_payload, role_key);

    let mut out: Vec<TierEntry> = raw
        .drain(..)
        .filter_map(|r| {
            let win_rate = r.win_num as f64 / r.win_den as f64;
            if !(0.0..=1.0).contains(&win_rate) {
                return None;
            }
            let pick_rate = if pick_sum > 0 {
                r.pick_weight as f64 / pick_sum as f64
            } else {
                0.0
            };
            if pick_rate < MIN_TIER_PICK_RATE {
                return None;
            }
            let mut provenance = overlay_types::recommendation::DataProvenance::now("ugg");
            provenance.sample_window = Some("current-patch".into());
            Some(TierEntry {
                champion_id: r.champion_id,
                win_rate,
                win_rate_delta: previous
                    .get(&r.champion_id)
                    .map(|prev| (win_rate - prev) * 100.0),
                games: Some(r.win_den),
                pick_rate,
                ban_rate: if ban_sum > 0 {
                    r.ban_weight as f64 / ban_sum as f64
                } else {
                    0.0
                },
                provenance,
            })
        })
        .collect();

    if out.is_empty() {
        return Err(ProviderError::NotEnoughData);
    }

    out.sort_by(|a, b| {
        b.win_rate
            .partial_cmp(&a.win_rate)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_lcu_roles_to_ugg_keys() {
        assert_eq!(tier_list_role_key("middle"), Some("mid"));
        assert_eq!(tier_list_role_key("utility"), Some("supp"));
    }

    #[test]
    fn parses_role_rows_from_fixture() {
        let payload = json!([{
            "jungle": [
                ["64", [], 15084, 31141, 0, 0, 100, 200, 300, 0],
                ["104", [], 20000, 40000, 0, 0, 50, 100, 150, 0]
            ]
        }, {}, "", 0.0]);

        let previous = json!([{
            "jungle": [
                ["64", [], 17000, 34000, 0, 0, 90, 200, 300, 0],
                ["104", [], 18000, 40000, 0, 0, 50, 100, 150, 0]
            ]
        }, {}, "", 0.0]);

        let rows = tier_entries_from_ranking(&payload, Some(&previous), "jungle").expect("parse");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].champion_id, 104);
        assert_eq!(rows[1].champion_id, 64);
        let lee = rows.iter().find(|r| r.champion_id == 64).unwrap();
        assert!((lee.win_rate - 0.4844).abs() < 0.001);
        assert!((lee.win_rate_delta.expect("previous win rate") + 1.56).abs() < 0.01);
        assert!((lee.pick_rate - 100.0 / 150.0).abs() < 0.001);
        assert!((lee.ban_rate - 300.0 / 450.0).abs() < 0.001);
        assert_eq!(lee.games, Some(31141));
    }

    #[test]
    fn filters_low_pick_rows() {
        let payload = json!([{
            "jungle": [
                ["64", [], 15084, 31141, 0, 0, 1000, 0, 0, 0],
                ["999", [], 50, 100, 0, 0, 1, 0, 0, 0]
            ]
        }, {}, "", 0.0]);

        let rows = tier_entries_from_ranking(&payload, None, "jungle").expect("parse");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].champion_id, 64);
    }
}
