# LOL.PS Windows / LCU acceptance

LOL.PS itself is ordinary HTTPS and is covered by the ignored live test. Rune
and summoner-spell import still requires Windows with a running League client.

## Preconditions

- Run the app on Windows with a logged-in League client.
- Use a practice/custom lobby so import behavior can be checked safely.
- Ensure the selected champion and role have a non-zero KR Emerald+ sample.

## Procedure

1. Open Settings and select `LOL.PS` under **BUILD DATA**.
2. Enter champion select, choose Gangplank, and select Top.
3. Confirm Item, Skill, Rune, Counter, and Tier sections populate and display
   `LOL.PS` as their active source.
4. Trigger manual build import with summoner-spell import enabled.
5. In the League client, confirm the created rune page has four primary perks,
   two secondary perks, three stat shards, and the two displayed summoner spells.
6. Switch **PLAYER STATS** and confirm LOL.PS is not offered there.

## Expected result

- Build panels use LOL.PS data without switching to another provider.
- Rune and spell import completes through the existing LCU command path.
- A known role with no sample shows the existing not-enough-data state instead
  of silently using a different role.
- Matchup-specific rune requests show not enough data because the anonymous
  LOL.PS contract does not expose them.
