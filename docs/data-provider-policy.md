# Data Provider Policy

This document defines how runtime data-source selection and fallback behavior
should work. The goal is to keep provider behavior predictable when switching
between DeepLoL and u.gg.

## Provider Selection

- `ProviderProxy` routes every `BuildProvider` call to the currently active
  provider only.
- Do not automatically retry a failed call against another provider.
- Data-source switching is explicit: `set_data_source` changes the active
  provider and emits `data-source` so frontend caches clear.
- `NotEnoughData` is a valid provider result. The UI should render its empty
  state instead of silently changing providers.

## Allowed Fallbacks

Fallback is allowed only inside the active provider, when it preserves the same
provider's semantics.

- Current patch -> previous patch is allowed for sparse patch-start data.
- Region fallback inside a provider is allowed only when that provider's data
  model already exposes a clear fallback target. u.gg overview/matchups use the
  selected platform region, then World.
- Role fallback inside a provider is allowed when the requested role is absent:
  use the role with the most games.
- Display fallback is allowed at the UI boundary: `NotEnoughData` can become a
  visible "Not enough data" state.

## Disallowed Fallbacks

- u.gg failure must not call DeepLoL automatically.
- DeepLoL failure must not call u.gg automatically.
- A provider must not mix another provider's rune, counter, tier-list, or item
  data into one response.
- A command handler must not hide provider errors by querying another provider.

## u.gg Specifics

- Default platform region is KR until `set_platform_id` provides another
  platform.
- Normal rune builds use `overview/{patch}/{mode}/{champion_key}`.
- Matchup rune builds use
  `overview/{patch}/{mode}/matchups/{champion_key}_{enemy_key}` and return
  `matchup: true`.
- Tier lists use `champion_ranking/{region}/{patch}/{mode}/emerald_plus`.
- Tier-list deltas are best effort: if the previous patch ranking cannot be
  fetched, rows still return with `win_rate_delta = 0.0`.
- Counters use current patch matchups first. If the filtered result is empty,
  previous patch matchups may be used. If previous patch lookup fails, return
  the current patch result.

## Mock Data

Mock champ-select scenarios may use hardcoded offline data when the active
provider cannot produce a scenario. That is a debug/mock fallback, not a runtime
provider fallback for HEXGATE commands.
