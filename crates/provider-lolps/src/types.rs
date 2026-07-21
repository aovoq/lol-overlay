use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VersionInfo {
    pub version_id: i64,
    pub description: String,
    pub patch_date: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct SummaryResponse {
    pub data: Vec<SummaryRow>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SummaryRow {
    pub build_type_id: i64,
    pub champion_id: i64,
    pub lane_id: i64,
    pub count: i64,
    pub win_rate: String,
    pub counter_champion_id_list: Vec<i64>,
    pub counter_winrate_list: Vec<f64>,
    pub counter_count_list: Vec<i64>,
    pub main_rune_category: Option<i64>,
    pub sub_rune_category: Option<i64>,
    pub main_rune1: Option<i64>,
    pub main_rune2: Option<i64>,
    pub main_rune3: Option<i64>,
    pub main_rune4: Option<i64>,
    pub sub_rune1: Option<i64>,
    pub sub_rune2: Option<i64>,
    pub statperk1_id: Option<i64>,
    pub statperk2_id: Option<i64>,
    pub statperk3_id: Option<i64>,
    pub spell1_id: Option<i64>,
    pub spell2_id: Option<i64>,
    pub skill_master_list: Vec<String>,
    pub skill_lv15_list: Vec<String>,
    pub starting_item_id_list: Vec<Vec<i64>>,
    pub core_item_id_list: Vec<i64>,
    pub shoes_id: Option<i64>,
    pub rune_total_winrate: String,
    pub rune_total_count: i64,
    pub skill_master_winrate: String,
    pub skill_master_count: i64,
    #[serde(default)]
    pub top1_three_core_winrate: Option<String>,
    #[serde(default)]
    pub top1_three_core_count: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct TierResponse {
    pub data: Vec<TierRow>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TierRow {
    pub champion_id: i64,
    pub lane_id: i64,
    pub count: i64,
    pub win_rate: String,
    pub pick_rate: String,
    pub ban_rate: String,
}

#[derive(Debug, Clone)]
pub(crate) struct Selected<T> {
    pub value: T,
    pub version: VersionInfo,
    pub fallback_from: Option<String>,
}
