import { A, useLocation, useNavigate } from "@solidjs/router";
import { createEffect, type JSX } from "solid-js";
import { automaticRoute } from "../../lib/navigation";
import { champSelect, phase } from "../../state/backend";
import { autoOpenDraft, autoOpenLive } from "../../state/settings";

const links = [
  { href: "/", label: "HOME", end: true },
  { href: "/draft", label: "DRAFT" },
  { href: "/summoners", label: "SUMMONERS" },
  { href: "/champions", label: "CHAMPIONS" },
  { href: "/live", label: "LIVE" },
  { href: "/settings", label: "SETTINGS" },
] as const;

export function DesktopShell(props: { children?: JSX.Element }) {
  const navigate = useNavigate();
  const location = useLocation();
  let routedDraft = false;
  let routedInGame = false;

  createEffect(() => {
    const active = champSelect()?.active ?? false;
    const inGame = phase()?.inGame ?? false;
    const route = automaticRoute({
      champSelectActive: active,
      inGame,
      routedDraft,
      routedInGame,
      autoOpenDraft: autoOpenDraft(),
      autoOpenLive: autoOpenLive(),
    });
    if (route) navigate(route);
    // Latch on the observed location, not the navigate() intent: a navigate
    // fired while the router is still resolving its initial location is
    // silently dropped, and the steady stream of phase/champ-select events
    // retries it here until the route actually sticks.
    if (active && location.pathname === "/draft") routedDraft = true;
    if (inGame && location.pathname === "/live") routedInGame = true;
    if (!active) routedDraft = false;
    if (!inGame) routedInGame = false;
  });

  const liveDot = (href: string) => {
    if (href === "/draft") return champSelect()?.active ?? false;
    if (href === "/live") return phase()?.inGame ?? false;
    return false;
  };

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
              {liveDot(link.href) ? <span class="desktop-live-dot" /> : null}
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
