export interface AutomaticNavigationState {
  champSelectActive: boolean;
  inGame: boolean;
  routedDraft: boolean;
  routedInGame: boolean;
  autoOpenDraft: boolean;
  autoOpenLive: boolean;
}

export function automaticRoute(state: AutomaticNavigationState): string | null {
  if (state.autoOpenLive && state.inGame && !state.routedInGame) return "/live";
  if (state.autoOpenDraft && state.champSelectActive && !state.routedDraft && !state.inGame) {
    return "/draft";
  }
  return null;
}
