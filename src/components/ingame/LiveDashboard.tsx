import { createMemo, For, Show } from "solid-js";
import { assetsReady, champIconByKey, champIconByName, champName, itemIconUrl } from "../../assets";
import { roleLabel } from "../../lib/openlol";
import { gamePlayers, lastDraft, recommendations } from "../../state/backend";
import { Icon } from "../Icon";
import { RecommendedItems } from "./RecommendedItems";
import { SkillOrder } from "./SkillOrder";
import { ThreatChips } from "./ThreatChips";

interface TeamRowData {
  key: string;
  iconUrl: string;
  /** Summoner/champion display name. */
  title: string;
  /** Secondary line (position / champion name). */
  sub: string;
  isSelf: boolean;
  isLaneOpponent: boolean;
  items: number[];
}

function positionLabel(position: string): string {
  return position ? roleLabel(position.toLowerCase()) : "";
}

/** One team column; live player identities, falling back to the retained
 * draft composition before the game client serves the Live Client API. */
function TeamColumn(props: { title: string; meta: string; rows: TeamRowData[]; enemy?: boolean }) {
  return (
    <section class="desktop-card live-team-card">
      <div class="desktop-section-heading">
        <h2>{props.title}</h2>
        <span>{props.meta}</span>
      </div>
      <div class="live-player-list">
        <For
          each={props.rows}
          fallback={<div class="live-team-empty">メンバー情報を待っています…</div>}
        >
          {(row) => (
            <div
              class={`live-player-row ${row.isSelf ? "is-self" : ""} ${
                row.isLaneOpponent ? "is-lane-opponent" : ""
              }`}
            >
              <Show when={assetsReady()}>
                <Icon url={row.iconUrl} class="live-player-icon" title={row.sub || row.title} />
              </Show>
              <div class="live-player-copy">
                <strong>{row.title}</strong>
                <small>{row.sub}</small>
              </div>
              <Show when={row.isLaneOpponent}>
                <span class="live-lane-badge">対面</span>
              </Show>
              <Show when={props.enemy && row.items.length > 0}>
                <span class="live-player-items">
                  <For each={row.items}>
                    {(itemId) => (
                      <Icon
                        url={itemIconUrl(itemId)}
                        class="live-player-item"
                        title={`#${itemId}`}
                      />
                    )}
                  </For>
                </span>
              </Show>
            </div>
          )}
        </For>
      </div>
    </section>
  );
}

export function LiveDashboard() {
  const recs = () => recommendations();
  const selfRawName = () => recs()?.selfRawName ?? "";
  const selfPos = () => (recs()?.selfPosition ?? "").toLowerCase();

  const allies = createMemo(() => (gamePlayers() ?? []).filter((p) => p.ally));
  const enemies = createMemo(() => (gamePlayers() ?? []).filter((p) => !p.ally));
  /** Recommendation enemy data (items included) keyed by raw champion name. */
  const recEnemyByRaw = createMemo(
    () => new Map((recs()?.enemies ?? []).map((e) => [e.rawName, e])),
  );

  const allyRows = createMemo((): TeamRowData[] => {
    const players = allies();
    if (players.length > 0) {
      return players.map((p) => ({
        key: p.rawName,
        iconUrl: champIconByName(p.rawName),
        title: p.riotId.split("#")[0] || p.name || p.rawName,
        sub: positionLabel(p.position),
        isSelf: p.rawName === selfRawName(),
        isLaneOpponent: false,
        items: [],
      }));
    }
    // Load screen: identities unknown yet — show the retained draft picks.
    return (lastDraft()?.myTeamChampionIds ?? [])
      .filter((id) => id > 0)
      .map((id) => ({
        key: String(id),
        iconUrl: champIconByKey(id),
        title: champName(id) || `#${id}`,
        sub: "",
        isSelf: id === (lastDraft()?.myChampionId ?? 0),
        isLaneOpponent: false,
        items: [],
      }));
  });

  const enemyRows = createMemo((): TeamRowData[] => {
    const lane = (position: string) => selfPos() !== "" && position.toLowerCase() === selfPos();
    const players = enemies();
    if (players.length > 0) {
      return players.map((p) => ({
        key: p.rawName,
        iconUrl: champIconByName(p.rawName),
        title: p.riotId.split("#")[0] || p.name || p.rawName,
        sub: positionLabel(p.position),
        isSelf: false,
        isLaneOpponent: lane(p.position),
        items: recEnemyByRaw().get(p.rawName)?.items ?? [],
      }));
    }
    const recEnemies = recs()?.enemies ?? [];
    if (recEnemies.length > 0) {
      return recEnemies.map((e) => ({
        key: e.rawName,
        iconUrl: champIconByName(e.rawName),
        title: e.name,
        sub: positionLabel(e.position),
        isSelf: false,
        isLaneOpponent: lane(e.position),
        items: e.items,
      }));
    }
    return (lastDraft()?.enemyChampionIds ?? [])
      .filter((id) => id > 0)
      .map((id) => ({
        key: String(id),
        iconUrl: champIconByKey(id),
        title: champName(id) || `#${id}`,
        sub: "",
        isSelf: false,
        isLaneOpponent: false,
        items: [],
      }));
  });

  return (
    <div class="live-dashboard">
      <TeamColumn title="味方チーム" meta="MY TEAM" rows={allyRows()} />

      <section class="desktop-card live-self-card">
        <Show
          when={recs()}
          fallback={<div class="live-team-empty">おすすめデータを読み込んでいます…</div>}
        >
          {(e) => (
            <>
              <div class="live-self-head">
                <Show when={assetsReady()}>
                  <Icon url={champIconByName(e().selfRawName)} class="live-self-icon" />
                </Show>
                <div class="live-player-copy">
                  <strong>{e().selfChampion || "—"}</strong>
                  <small>{positionLabel(e().selfPosition)}</small>
                </div>
                <ThreatChips threats={e().threats} />
              </div>
              <SkillOrder order={e().skillOrder} championImageId={e().selfRawName} />
              <div class="live-build-list">
                <div class="hx-section-title">RECOMMENDED BUILD</div>
                <RecommendedItems items={e().items} />
              </div>
            </>
          )}
        </Show>
      </section>

      <TeamColumn title="敵チーム" meta="ENEMY TEAM" rows={enemyRows()} enemy />
    </div>
  );
}
