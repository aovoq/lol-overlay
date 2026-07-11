export interface AutomaticNavigationState {
  championId: number;
  championLocked: boolean;
  inGame: boolean;
  routedChampion: number;
  routedInGame: boolean;
  autoOpenChampion: boolean;
  autoOpenLive: boolean;
}

export function automaticRoute(state: AutomaticNavigationState): string | null {
  if (state.autoOpenLive && state.inGame && !state.routedInGame) return "/live";
  if (
    state.autoOpenChampion &&
    state.championLocked &&
    state.championId > 0 &&
    state.championId !== state.routedChampion
  ) {
    return `/champions/${state.championId}`;
  }
  return null;
}
