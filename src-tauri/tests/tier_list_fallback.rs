//! Live u.gg tier list via stats2 `champion_ranking`.

use std::sync::Arc;

use overlay_ddragon::DdragonClient;
use overlay_provider::{BuildProvider, BuildProviderProxy, ProviderKind};
use overlay_provider_deeplol::DeepLolProvider;
use overlay_provider_ugg::UggProvider;

#[tokio::test]
#[ignore = "network"]
async fn ugg_tier_list_from_champion_ranking() {
    let ddragon = Arc::new(DdragonClient::new());
    let deeplol: Arc<dyn BuildProvider> =
        Arc::new(DeepLolProvider::new(ddragon.clone()).expect("deeplol provider"));
    let ugg: Arc<dyn BuildProvider> = Arc::new(UggProvider::new(ddragon).expect("ugg provider"));
    let proxy = BuildProviderProxy::new(
        ProviderKind::Ugg,
        [(ProviderKind::Deeplol, deeplol), (ProviderKind::Ugg, ugg)],
    )
    .expect("proxy");
    proxy.set_active(ProviderKind::Ugg).unwrap();

    let rows = proxy
        .tier_list("jungle")
        .await
        .expect("ugg tier_list should succeed");
    assert!(
        rows.len() >= 5,
        "expected jungle tier rows from u.gg champion_ranking, got {}",
        rows.len()
    );
    assert!(
        rows.iter().all(|r| r.win_rate > 0.0 && r.win_rate <= 1.0),
        "win rates must be 0..1"
    );
    assert!(
        rows.windows(2).all(|w| w[0].win_rate >= w[1].win_rate),
        "tier list must be sorted by win rate desc"
    );
}
