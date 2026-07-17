# Counter layout design QA

- Source visual truth: `/Users/voq/Pictures/Screenshots/Screenshot 2026-07-17 at 10.53.04.png`
- Implementation screenshot: `/tmp/openlol-counter-qa-20260717/after-with-games-1316x793.png`
- Full-view comparison: `/tmp/openlol-counter-qa-20260717/before-after-exact.png`
- Focused comparison: `/tmp/openlol-counter-qa-20260717/counter-focused-before-after.png`
- Viewport: 1316 x 793 CSS pixels (the source screenshot is 2632 x 1586 at 2x density)
- State: Gwen champion detail, MID selected, eight populated counter entries. The browser preview uses `counter-mock`; Tauri-only rune/build requests remain unavailable in the browser and are excluded from this focused layout verdict.

## Findings

- No remaining P0/P1/P2 issue in the counter list.
- Fonts and typography: percentages retain their size, color, and alignment after wrapping; game counts use a smaller secondary line and the `WR · GAMES` legend identifies both metrics.
- Spacing and layout rhythm: the former single-row overflow now wraps to five entries plus three entries with a consistent horizontal and vertical gap.
- Colors and visual tokens: unchanged; existing border, muted text, and positive win-rate tokens are preserved.
- Image quality and asset fidelity: official champion icons continue to use the existing `Icon` and Data Dragon asset path without scaling or crop changes.
- Copy and content: unchanged apart from mock champion/value choices used only for browser QA.

## Comparison history

1. Before: P1 layout failure. Eight fixed-width counter entries plus gaps exceeded the 280px detail rail and painted over the adjacent build card.
2. Fix: enabled wrapping on the shared `Counters` list, kept each counter and loading skeleton at a non-shrinking fixed width, and allowed the component root to shrink inside its grid track.
3. After: all eight entries are contained within the counter card in a five-plus-three layout. Each entry shows win rate and compact game count, and no icon, metric, or persistent control crosses the card boundary.

## Implementation checklist

- [x] Champion detail counter list wraps inside the narrow detail rail.
- [x] Draft reuses the same corrected component.
- [x] Loading skeletons follow the same wrapping constraint.
- [x] Verified with eight realistic entries at the source viewport.
- [x] Each counter exposes both win rate and game count, with a visible metric legend.

## Follow-up polish

- None required for this defect.

final result: passed
