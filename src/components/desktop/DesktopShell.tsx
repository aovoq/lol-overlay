import { A, useNavigate } from "@solidjs/router";
import { createEffect, type JSX } from "solid-js";
import { automaticRoute } from "../../lib/navigation";
import { champSelect, phase, setSelectedRole } from "../../state/backend";
import { autoOpenChampion, autoOpenLive } from "../../state/settings";

const links = [
  { href: "/", label: "HOME", end: true },
  { href: "/champions", label: "CHAMPIONS" },
  { href: "/live", label: "LIVE" },
  { href: "/settings", label: "SETTINGS" },
] as const;

export function DesktopShell(props: { children?: JSX.Element }) {
  const navigate = useNavigate();
  let routedChampion = 0;
  let routedInGame = false;

  createEffect(() => {
    const draft = champSelect();
    const inGame = phase()?.inGame ?? false;
    const route = automaticRoute({
      championId: draft?.myChampionId ?? 0,
      championLocked: draft?.myLocked ?? false,
      inGame,
      routedChampion,
      routedInGame,
      autoOpenChampion: autoOpenChampion(),
      autoOpenLive: autoOpenLive(),
    });
    if (route?.startsWith("/champions/")) {
      routedChampion = draft?.myChampionId ?? 0;
      if (draft?.myRole) setSelectedRole(draft.myRole);
    } else if (route === "/live") {
      routedInGame = true;
    }
    if (route) navigate(route);
    if (!draft?.active) routedChampion = 0;
    if (!inGame) routedInGame = false;
  });

  return (
    <div class="desktop-shell">
      <header class="desktop-topbar">
        <div class="desktop-brand">OPENLOL</div>
        <nav class="desktop-nav" aria-label="メインナビゲーション">
          {links.map((link) => (
            <A
              href={link.href}
              end={"end" in link ? link.end : false}
              class="desktop-nav-link"
              activeClass="desktop-nav-link--active"
            >
              <span>{link.label}</span>
              {link.href === "/live" && phase()?.inGame ? <span class="desktop-live-dot" /> : null}
            </A>
          ))}
        </nav>
        <div class="desktop-client-state">
          <span class={`desktop-state-dot ${phase()?.clientUp ? "is-online" : ""}`} />
          {phase()?.clientUp ? phase()?.phase || "CONNECTED" : "CLIENT OFFLINE"}
        </div>
      </header>
      <main class="desktop-content">{props.children}</main>
    </div>
  );
}
