//! Shared serialize-able types used across the overlay crates and frontend events.

pub mod champ_select;
pub mod lcu;
mod player;
pub mod recommendation;
pub mod snapshot;

pub use champ_select::ChampSelectEvent;
pub use lcu::{MatchmakingInfo, MyPick, Phase, RecentGame, RunePagePayload, SummonerInfo};
pub use player::{
    MatchFailure, MatchPage, MatchParticipant, PlayerChampionStats, PlayerIdentity, PlayerMatch,
    PlayerProfile, PlayerRef, ProviderExtras, RankedEntry, RefreshAvailability, RefreshResult,
    SeasonRank,
};
pub use recommendation::{
    CounterEntry, ItemRecommendation, RuneBuild, RuneRecommendation, SkillOrder, ThreatProfile,
    TierEntry,
};
pub use snapshot::{EnemyChampion, GamePlayer, GameSnapshot};

#[cfg(test)]
mod tests {
    use super::{ChampSelectEvent, RuneBuild, SummonerInfo};
    use serde_json::json;

    #[test]
    fn champ_select_event_uses_camel_case() {
        let event = ChampSelectEvent {
            active: true,
            my_role: "middle".into(),
            my_champion_id: 103,
            my_locked: true,
            my_team_champion_ids: vec![103],
            enemy_champion_ids: vec![238],
            my_bans: vec![0],
            enemy_bans: vec![0],
            timer_phase: "BAN_PICK".into(),
        };

        assert_eq!(
            serde_json::to_value(event).expect("json"),
            json!({
                "active": true,
                "myRole": "middle",
                "myChampionId": 103,
                "myLocked": true,
                "myTeamChampionIds": [103],
                "enemyChampionIds": [238],
                "myBans": [0],
                "enemyBans": [0],
                "timerPhase": "BAN_PICK"
            })
        );
    }

    #[test]
    fn recommendation_payloads_use_frontend_field_names() {
        let build = RuneBuild {
            page_name: "OPENLOL Ahri Middle".into(),
            lane: "Middle".into(),
            win_rate: 0.52,
            games: 123,
            primary_style_id: 8100,
            sub_style_id: 8200,
            primary_perk_ids: vec![8112, 8139, 8138, 8106],
            sub_perk_ids: vec![8226, 8210],
            shard_ids: vec![5008, 5008, 5011],
            spell_ids: vec![4, 14],
            matchup: false,
        };
        let value = serde_json::to_value(build).expect("json");

        assert_eq!(value["pageName"], "OPENLOL Ahri Middle");
        assert_eq!(value["winRate"], 0.52);
        assert_eq!(value["primaryPerkIds"], json!([8112, 8139, 8138, 8106]));
    }

    #[test]
    fn summoner_info_uses_camel_case() {
        let summoner = SummonerInfo {
            game_name: "Faker".into(),
            tag_line: "KR1".into(),
            level: 100,
            profile_icon_id: 1,
            solo_tier: "CHALLENGER".into(),
            solo_division: "NA".into(),
            solo_lp: 1000,
            solo_wins: 10,
            solo_losses: 2,
        };
        let value = serde_json::to_value(summoner).expect("json");

        assert_eq!(value["gameName"], "Faker");
        assert_eq!(value["profileIconId"], 1);
        assert_eq!(value["soloWins"], 10);
    }
}
