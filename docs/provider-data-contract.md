# Provider data contract

This policy defines the normalized meaning of build-provider output. Adapters may omit unknown
values, but must not substitute zero or silently mix another provider's data.

| Field | Normalized meaning | Unknown value | Validation |
| --- | --- | --- | --- |
| `winRate` | Wins divided by games for the stated population | Endpoint fails with `NotEnoughData` | finite, `0..=1` |
| `games` | Actual sample count unless provenance says `estimated` | `null` | non-negative |
| `winRateDelta` | Current minus comparable previous win rate, percentage points | `null` | finite |
| `pickRate`, `banRate` | Share of the stated population | Endpoint fails with `NotEnoughData` | finite, `0..=1` |
| Counter `winRate` | Counter champion's win rate against the subject | Row omitted | finite, `0..=1` |
| Region | Population region, not the logged-in platform | `null` | attached in provenance when disclosed |
| Sample window | Upstream time/patch window | `null` | attached in provenance when disclosed |

`DataProvenance` travels with tier rows and contains provider, region, patch, rank, sample window,
fetch time, estimation status, and fallback origin. The desktop tier UI exposes this metadata in
the row tooltip. Provider routing never supplies a fallback automatically.

## Provider differences

| Provider | Population | Games | Delta | Known fallback |
| --- | --- | --- | --- | --- |
| DeepLoL | KR current-patch rank data | Estimated from lane total and pick rate when calibration succeeds | Previous patch when available | Build data may explicitly fall back from player region to KR |
| U.GG | Current-patch ranking response | Upstream denominator | Previous response when comparable | Adapter may use documented region-to-World or role fallback; provenance must disclose it |
| OP.GG | Global tier response | Not exposed (`null`) | Not exposed (`null`) | None in the normalized adapter |
| LoLalytics | 30-day aggregate | Upstream games | No comparable baseline (`null`) | None in the normalized adapter |

All build results pass through the shared normalizer in `BuildProviderProxy`. Invalid values return
the typed `ProviderError::InvalidData`; they are never repaired into plausible-looking statistics.

