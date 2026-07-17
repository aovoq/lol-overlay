import { A, useLocation, useNavigate } from "@solidjs/router";
import { createEffect, type JSX, onMount, Show } from "solid-js";
import { automaticRoute } from "../../lib/navigation";
import { dataSourceLabel, phaseChipLabel } from "../../lib/openlol";
import { availableUpdateVersion } from "../../lib/updater";
import { champSelect, importState, phase } from "../../state/backend";
import { initMobilePairingState, mobilePairing } from "../../state/mobile";
import { autoImport, autoOpenDraft, autoOpenLive, dataSource } from "../../state/settings";

const navGroups = [
  {
    label: "PLAY",
    links: [
      { href: "/", label: "ホーム", meta: "OVERVIEW", end: true },
      { href: "/draft", label: "ドラフト", meta: "DRAFT" },
      { href: "/live", label: "試合中", meta: "LIVE" },
    ],
  },
  {
    label: "EXPLORE",
    links: [
      { href: "/summoners", label: "サモナー", meta: "SUMMONERS" },
      { href: "/champions", label: "チャンピオン", meta: "CHAMPIONS" },
    ],
  },
  {
    label: "SYSTEM",
    links: [{ href: "/settings", label: "設定", meta: "SETTINGS" }],
  },
] as const;

export function DesktopShell(props: { children?: JSX.Element }) {
  const navigate = useNavigate();
  const location = useLocation();
  let routedDraft = false;
  let routedInGame = false;

  onMount(() => initMobilePairingState());

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

  const clientStatus = () => {
    const current = phase();
    return current ? phaseChipLabel(current) : "CONNECTING";
  };
  const autoImportStatus = () => {
    if (!autoImport()) return "OFF";
    if (importState() === "importing") return "WORKING";
    if (importState() === "failed") return "FAILED";
    return "ON";
  };
  const autoImportTone = () => {
    if (importState() === "failed") return "is-error";
    if (autoImport()) return importState() === "importing" ? "is-active" : "is-ok";
    return "";
  };
  const mobileStatus = () => {
    if (mobilePairing().status === "paired") return "PAIRED";
    if (mobilePairing().status === "error") return "ERROR";
    return "OFF";
  };

  return (
    <div class="desktop-shell">
      <aside class="desktop-sidebar">
        <div class="desktop-brand">
          <strong>OPENLOL</strong>
          <small>LEAGUE COMPANION</small>
        </div>
        <nav class="desktop-nav" aria-label="メインナビゲーション">
          {navGroups.map((group) => (
            <section class="desktop-nav-group">
              <h2>{group.label}</h2>
              <div class="desktop-nav-group-links">
                {group.links.map((link) => (
                  <A
                    href={link.href}
                    end={"end" in link ? link.end : false}
                    class="desktop-nav-link"
                    activeClass="desktop-nav-link--active"
                  >
                    <span class="desktop-nav-copy">
                      <strong>{link.label}</strong>
                      <small>{link.meta}</small>
                    </span>
                    {liveDot(link.href) ? <span class="desktop-live-dot" /> : null}
                  </A>
                ))}
              </div>
            </section>
          ))}
        </nav>
        <div class="desktop-status-stack">
          <div class="desktop-client-state">
            <span class={`desktop-state-dot ${phase()?.clientUp ? "is-online" : ""}`} />
            <span>
              <small>LEAGUE CLIENT</small>
              <strong>{clientStatus()}</strong>
            </span>
          </div>
          <section class="desktop-status-rows" aria-label="アプリ状態">
            <div class="desktop-status-row">
              <span>AUTO IMPORT</span>
              <strong class={autoImportTone()}>{autoImportStatus()}</strong>
            </div>
            <div class="desktop-status-row">
              <span>MOBILE</span>
              <strong
                class={
                  mobilePairing().status === "paired"
                    ? "is-ok"
                    : mobilePairing().status === "error"
                      ? "is-error"
                      : ""
                }
              >
                {mobileStatus()}
              </strong>
            </div>
            <div class="desktop-status-row">
              <span>BUILD DATA</span>
              <strong>{dataSourceLabel(dataSource())}</strong>
            </div>
            <Show when={availableUpdateVersion()}>
              {(version) => (
                <div class="desktop-status-row is-attention">
                  <span>UPDATE</span>
                  <strong>v{version()}</strong>
                </div>
              )}
            </Show>
          </section>
        </div>
      </aside>
      <main class="desktop-content">{props.children}</main>
    </div>
  );
}
