import type { PhaseEvent } from "../types";

/** Whether the last champ-select context should stay on the draft board.
 *
 * Kept through the load screen and the game itself (that is when the draft
 * summary is most useful to look back at); dropped the moment the gameflow
 * returns to a pre-game phase — a dodge or the post-game lobby — so a stale
 * draft never leaks into the next queue. */
export function retainDraft(phase: PhaseEvent): boolean {
  return phase.inGame || phase.phase === "ChampSelect" || phase.phase === "InProgress";
}
