import { createEffect, createMemo, createSignal, For, Index, type JSX, Show } from "solid-js";
import {
  assetsReady,
  champIconByKey,
  champImageId,
  championKeyByImage,
  champName,
  fmtCompact,
  fmtPct,
} from "../../assets";
import { roleLabel } from "../../lib/openlol";
import {
  champSelect,
  gamePlayers,
  lastDraft,
  phase,
  selectedRole,
  setUserPickedVsEnemy,
  setVsEnemyId,
  userPickedVsEnemy,
  vsEnemyId,
} from "../../state/backend";
import { buildDetailsCache, buildKey, tierCache } from "../../state/caches";
import type { GamePlayer, TierEntry } from "../../types";
import { Icon } from "../Icon";
import { BuildArea } from "../openlol/BuildArea";
import { Counters } from "../openlol/Counters";
import { ImportButton } from "../openlol/ImportButton";
import { ItemPath } from "../openlol/ItemPath";
import { SectionError } from "../openlol/SectionError";
import { SkillMaster, SkillMatrix } from "../openlol/SkillMatrix";
import { StatsRow } from "../openlol/StatsRow";
import { ScrollArea } from "../ScrollArea";
import { RoleSelector } from "./Pages";

/** Champion previewed in the build column while hovering a tier-list row. */
const [hoverChampId, setHoverChampId] = createSignal(0);

/** Champ-select session to display: the live one while champ select runs,
 * then the retained snapshot through the load screen and the game. */
const draft = () => {
  const live = champSelect();
  return live?.active ? live : lastDraft();
};

function ChampRow(props: {
  championId: number;
  rank?: number;
  title?: string;
  children?: JSX.Element;
}) {
  return (
    <div
      class={`flex-none flex items-center gap-2 px-2 py-1 rounded-md transition-colors ${
        hoverChampId() === props.championId
          ? "bg-hx-accent-wash ring-1 ring-inset ring-hx-accent-dim"
          : "hover:bg-hx-bg-raised"
      }`}
      title={`ホバーで${champName(props.championId)}のルーンをプレビュー${props.title ? ` · ${props.title}` : ""}`}
      onMouseEnter={() => setHoverChampId(props.championId)}
      onMouseLeave={() => setHoverChampId(0)}
    >
      <Show when={props.rank !== undefined}>
        <span class="w-4 flex-none text-right text-[10px] font-bold text-hx-muted tabular-nums">
          {props.rank}
        </span>
      </Show>
      <Show when={assetsReady()}>
        <Icon
          url={champIconByKey(props.championId)}
          class="w-7 h-7 rounded border border-hx-border object-cover"
        />
      </Show>
      <span class="flex-1 min-w-0 overflow-hidden text-ellipsis whitespace-nowrap">
        {champName(props.championId) || `#${props.championId}`}
      </span>
      {props.children}
    </div>
  );
}

/** Column labels aligned with the row layout so a list reads as a table. */
function ColHeader(props: { rank?: boolean; cols: { label: string; class: string }[] }) {
  return (
    <div class="flex items-center gap-2 px-2 text-[9px] font-bold tracking-[0.12em] text-hx-muted tabular-nums">
      <Show when={props.rank}>
        <span class="w-4 flex-none" />
      </Show>
      <span class="flex-1" />
      <For each={props.cols}>{(c) => <span class={`${c.class} text-right`}>{c.label}</span>}</For>
    </div>
  );
}

function StrongRow(props: { entry: TierEntry; rank: number }) {
  const t = () => props.entry;
  const delta = () => {
    const d = t().winRateDelta ?? 0;
    if (Math.abs(d) < 0.5) return null;
    return `${d > 0 ? "▲" : "▼"}${Math.abs(d).toFixed(1)}`;
  };

  return (
    <ChampRow
      championId={t().championId}
      rank={props.rank}
      title={`${t().provenance.provider} · ${t().provenance.region ?? "region unknown"} · ${t().provenance.patch ?? t().provenance.sampleWindow ?? "sample unknown"}${t().provenance.fallbackFrom ? ` · fallback from ${t().provenance.fallbackFrom}` : ""}`}
    >
      <span class="w-12 text-right font-bold text-hx-text">{fmtPct(t().winRate)}</span>
      <span
        class={`w-[38px] text-right text-[11px] ${
          delta() ? ((t().winRateDelta ?? 0) > 0 ? "text-hx-up" : "text-hx-red") : ""
        }`}
      >
        {delta()}
      </span>
      <span class="w-11 text-right text-xs text-hx-muted">
        {(t().games ?? 0) > 0 ? fmtCompact(t().games ?? 0) : fmtPct(t().pickRate)}
      </span>
    </ChampRow>
  );
}

function BanRow(props: { entry: TierEntry; rank: number }) {
  const t = () => props.entry;
  return (
    <ChampRow
      championId={t().championId}
      rank={props.rank}
      title={`${t().provenance.provider} · ${t().provenance.region ?? "region unknown"} · ${t().provenance.patch ?? t().provenance.sampleWindow ?? "sample unknown"}`}
    >
      <span class="w-12 text-right font-bold text-hx-red">{fmtPct(t().winRate)}</span>
      <span class="w-11 text-right text-xs text-hx-muted">{fmtPct(t().pickRate)}</span>
      <span class="w-11 text-right text-xs text-hx-muted">{fmtPct(t().banRate)}</span>
    </ChampRow>
  );
}

function SkeletonRows(props: { count: number }) {
  return (
    <For each={Array.from({ length: props.count }, (_, i) => i)}>
      {() => <div class="flex-none hx-skel h-7 rounded-md" />}
    </For>
  );
}

function TierLists(props: { role: string }) {
  const bannedIds = createMemo(() => {
    const cs = draft();
    return new Set([...(cs?.myBans ?? []), ...(cs?.enemyBans ?? [])].filter((id) => id > 0));
  });
  const entry = createMemo(() => tierCache.get(props.role));

  // Clear the preview when the list contents shift under the cursor.
  createEffect(() => {
    props.role;
    entry().state;
    bannedIds();
    assetsReady();
    setHoverChampId(0);
  });

  const strong = createMemo(() => {
    const e = entry();
    if (e.state !== "ok") return [];
    return e.value.filter((t) => t.pickRate >= 0.005).sort((a, b) => b.winRate - a.winRate);
  });

  const bans = createMemo(() => {
    const e = entry();
    if (e.state !== "ok") return [];
    const banned = bannedIds();
    return e.value
      .filter((t) => !banned.has(t.championId))
      .sort((a, b) => (b.winRate - 0.5) * b.pickRate - (a.winRate - 0.5) * a.pickRate)
      .slice(0, 10);
  });

  const errMsg = createMemo(() => {
    const e = entry();
    return e.state === "err" ? e.error : "";
  });
  const isLoading = createMemo(() => entry().state === "loading");
  const isOk = createMemo(() => entry().state === "ok");

  return (
    <>
      <div class="flex items-baseline justify-between gap-2 px-0.5 pt-1 pb-0.5">
        <span class="hx-section-title">STRONG PICKS</span>
        <span class="flex min-w-0 flex-col items-end gap-0.5">
          <span class="text-[10px] font-bold tracking-[0.12em] text-hx-muted">
            {roleLabel(props.role)}
          </span>
          <span class="text-[7px] font-bold tracking-[0.1em] text-hx-accent-dim">
            HOVER TO PREVIEW
          </span>
        </span>
      </div>
      <ColHeader
        rank
        cols={[
          { label: "WR", class: "w-12" },
          { label: "Δ", class: "w-[38px]" },
          { label: "GAMES", class: "w-11" },
        ]}
      />
      <ScrollArea class="min-h-0 flex-[1.3_1_0]" contentClass="flex flex-col gap-0.5 pr-1">
        <Show
          when={isLoading()}
          fallback={
            <Show
              when={isOk()}
              fallback={
                <SectionError message={errMsg()} onRetry={() => tierCache.refetch(props.role)} />
              }
            >
              <For each={strong()}>{(t, i) => <StrongRow entry={t} rank={i() + 1} />}</For>
            </Show>
          }
        >
          <SkeletonRows count={8} />
        </Show>
      </ScrollArea>

      <div class="hx-section-title px-0.5 pt-2 pb-0.5">BAN TARGETS</div>
      <ColHeader
        rank
        cols={[
          { label: "WR", class: "w-12" },
          { label: "PICK", class: "w-11" },
          { label: "BAN", class: "w-11" },
        ]}
      />
      <ScrollArea class="min-h-0 flex-1" contentClass="flex flex-col gap-0.5 pr-1">
        <Show
          when={isLoading()}
          fallback={
            <Show when={isOk()}>
              <For each={bans()}>{(t, i) => <BanRow entry={t} rank={i() + 1} />}</For>
            </Show>
          }
        >
          <SkeletonRows count={4} />
        </Show>
      </ScrollArea>
    </>
  );
}

function TeamRow(props: {
  label: string;
  ids: number[];
  enemy?: boolean;
  /** Load-screen identities keyed by champion id (empty before the game). */
  players?: Map<number, GamePlayer>;
}) {
  const slots = createMemo(() => (props.ids.length ? props.ids : [0, 0, 0, 0, 0]));
  const showNames = createMemo(() => (props.players?.size ?? 0) > 0);

  return (
    <div>
      <div class="mb-1.5 flex items-center justify-between gap-2 text-[9px] font-bold tracking-[0.18em] text-hx-muted">
        <span>{props.label}</span>
        <Show when={props.enemy}>
          <span class="text-[7px] tracking-[0.08em] text-hx-accent-dim">CLICK TO SET MATCHUP</span>
        </Show>
      </div>
      <div class="flex gap-2">
        <Index each={slots()}>
          {(id) => {
            const player = () => (id() > 0 ? props.players?.get(id()) : undefined);
            return (
              <div class="flex flex-col items-center gap-0.5 w-11 min-w-0">
                <button
                  type="button"
                  class={`w-10 h-10 flex items-center justify-center bg-hx-bg-raised border rounded-md overflow-hidden font-hx-display text-[15px] text-hx-muted ${
                    props.enemy && id() > 0 ? "cursor-pointer" : "cursor-default"
                  } ${
                    props.enemy && id() > 0 && id() === vsEnemyId()
                      ? "border-hx-accent ring-1 ring-hx-accent"
                      : id() > 0
                        ? props.enemy
                          ? "border-hx-red"
                          : "border-hx-border"
                        : "border-hx-border"
                  }`}
                  disabled={!props.enemy || id() <= 0}
                  aria-pressed={props.enemy && id() > 0 ? id() === vsEnemyId() : undefined}
                  title={
                    id() > 0
                      ? props.enemy
                        ? `${champName(id())} · クリックで対面に設定`
                        : champName(id())
                      : undefined
                  }
                  onClick={() => {
                    if (!props.enemy || id() <= 0) return;
                    // Clicking the selected enemy again returns to the best build.
                    setVsEnemyId(id() === vsEnemyId() ? 0 : id());
                    setUserPickedVsEnemy(true);
                  }}
                >
                  <Show when={id() > 0 && assetsReady()} fallback="?">
                    <Icon url={champIconByKey(id())} class="w-full h-full object-cover" />
                  </Show>
                </button>
                <Show when={showNames()}>
                  <span
                    class="w-full text-center text-[8px] leading-tight text-hx-muted overflow-hidden text-ellipsis whitespace-nowrap"
                    title={player()?.riotId}
                  >
                    {player()?.riotId.split("#")[0] || " "}
                  </span>
                </Show>
              </div>
            );
          }}
        </Index>
      </div>
    </div>
  );
}

function BansRow() {
  const banned = createMemo(() => {
    const cs = draft();
    return [...new Set([...(cs?.myBans ?? []), ...(cs?.enemyBans ?? [])])].filter((id) => id > 0);
  });

  return (
    <Show when={banned().length > 0}>
      <div>
        <div class="text-[9px] font-bold tracking-[0.18em] text-hx-muted mb-1.5">BANS</div>
        <div class="flex gap-1.5 flex-wrap">
          <For each={banned()}>
            {(id) => (
              <Show when={assetsReady()}>
                <Icon
                  url={champIconByKey(id)}
                  class="w-7 h-7 rounded border border-hx-border object-cover grayscale opacity-70"
                  title={champName(id)}
                />
              </Show>
            )}
          </For>
        </div>
      </div>
    </Show>
  );
}

function MatchupLine(props: { role: string }) {
  const my = createMemo(() => draft()?.myChampionId ?? 0);
  const enemy = createMemo(() => vsEnemyId());

  return (
    <Show when={my()}>
      <div class="flex items-center gap-1.5 text-[13px]">
        <span class="font-hx-display font-semibold text-[11px] tracking-[0.16em] text-hx-muted">
          {roleLabel(props.role)} ·{" "}
        </span>
        <Show when={assetsReady()}>
          <Icon
            url={champIconByKey(my())}
            class="w-5 h-5 rounded border border-hx-border object-cover"
          />
        </Show>
        <span class="text-hx-accent font-semibold">{champName(my()) || `#${my()}`}</span>
        <Show when={enemy()}>
          <span class="text-hx-muted italic">vs</span>
          <Show when={assetsReady()}>
            <Icon
              url={champIconByKey(enemy())}
              class="w-5 h-5 rounded border border-hx-border object-cover"
            />
          </Show>
          <span class="text-hx-text">{champName(enemy()) || `#${enemy()}`}</span>
          <button
            type="button"
            class="ml-1.5 border border-hx-border rounded px-2 py-0.5 font-hx-display font-semibold text-[9px] tracking-[0.16em] text-hx-muted hover:text-hx-accent hover:border-hx-accent-dim cursor-pointer"
            onClick={() => {
              setVsEnemyId(0);
              setUserPickedVsEnemy(true);
            }}
          >
            BEST BUILD
          </button>
        </Show>
      </div>
    </Show>
  );
}

const TIMER_PHASE_LABELS: Record<string, string> = {
  PLANNING: "PLANNING",
  BAN_PICK: "BAN / PICK",
  FINALIZATION: "FINALIZATION",
  GAME_STARTING: "GAME STARTING",
};

export function DraftPage() {
  const cs = () => draft();
  const active = () => champSelect()?.active ?? false;
  /** True while showing the retained draft (load screen / in-game). */
  const stale = () => !active() && !!cs();
  const myRole = () => cs()?.myRole ?? "";
  const role = () => myRole() || selectedRole();
  const myChampionId = () => cs()?.myChampionId ?? 0;
  const revealedEnemies = createMemo(() => (cs()?.enemyChampionIds ?? []).filter((id) => id > 0));

  // Load-screen identities keyed by champion id (empty until the game client
  // starts serving the Live Client API).
  const playerByChampion = createMemo(() => {
    const players = gamePlayers();
    const map = new Map<number, GamePlayer>();
    if (!players || !assetsReady()) return map;
    for (const player of players) {
      const key = championKeyByImage(player.rawName);
      if (key) map.set(key, player);
    }
    return map;
  });

  // Once live positions are known, the lane opponent is definitive.
  const confirmedLaneEnemy = createMemo(() => {
    const players = gamePlayers();
    const pos = role().toLowerCase();
    if (!players || !pos || !assetsReady()) return 0;
    const enemy = players.find((p) => !p.ally && p.position.toLowerCase() === pos);
    return enemy ? championKeyByImage(enemy.rawName) : 0;
  });

  // Default matchup: among revealed enemies, the one most likely to share our
  // lane — approximated by the highest pick rate in our role.
  const likelyLaneEnemy = createMemo(() => {
    const revealed = revealedEnemies();
    if (!revealed.length) return 0;
    const entry = tierCache.get(role());
    if (entry.state !== "ok") return revealed[0];
    const pickRates = new Map(entry.value.map((t) => [t.championId, t.pickRate]));
    return (
      revealed
        .map((id) => ({ id, pickRate: pickRates.get(id) ?? -1 }))
        .sort((a, b) => b.pickRate - a.pickRate)[0]?.id ?? revealed[0]
    );
  });

  createEffect(() => {
    const revealed = revealedEnemies();
    if (!revealed.length) {
      setVsEnemyId(0);
      setUserPickedVsEnemy(false);
      return;
    }
    // A user-made choice sticks: an explicit enemy until they leave the
    // draft, "best build" (0) indefinitely. Otherwise follow the confirmed
    // lane opponent (live positions), falling back to the pick-rate guess.
    if (userPickedVsEnemy() && (vsEnemyId() === 0 || revealed.includes(vsEnemyId()))) return;
    const confirmed = confirmedLaneEnemy();
    setVsEnemyId(confirmed && revealed.includes(confirmed) ? confirmed : likelyLaneEnemy());
    setUserPickedVsEnemy(false);
  });

  // Hovering a tier-list row previews that champion's best build; otherwise
  // the column follows our own pick (hover intent included) and matchup.
  const previewChamp = () => hoverChampId() || myChampionId();
  const previewEnemy = () => (hoverChampId() ? null : vsEnemyId() || null);

  // Skill order + items only for our own champion — a hover preview shows
  // that champion's runes, so rendering our details next to them would lie.
  const detailsEntry = createMemo(() => {
    const id = myChampionId();
    if (id <= 0 || hoverChampId()) return null;
    return buildDetailsCache.get(buildKey(id, role(), vsEnemyId() || null));
  });
  const detailsValue = createMemo(() => {
    const e = detailsEntry();
    return e?.state === "ok" ? e.value : null;
  });

  const statusLabel = () => {
    if (active()) return TIMER_PHASE_LABELS[champSelect()?.timerPhase ?? ""] ?? "CHAMP SELECT";
    if (stale()) return phase()?.inGame ? "IN GAME" : "LOADING";
    return "STANDBY";
  };

  return (
    <ScrollArea class="desktop-draft-scroll" contentClass="desktop-page desktop-draft">
      <header class="desktop-page-header">
        <span class="desktop-eyebrow">DRAFT</span>
        <div class="desktop-draft-title-row">
          <h1>ドラフト</h1>
          <span class={`desktop-draft-status ${active() ? "is-active" : ""}`}>{statusLabel()}</span>
        </div>
        <p>
          {active()
            ? "チャンプセレクト進行中。リストにホバーするとルーンをプレビューできます。"
            : stale()
              ? "この試合のドラフトを表示中。ロード画面以降はサモナーネームも確認できます。"
              : "チャンプセレクト待機中。ロールを選んでピック候補を下調べできます。"}
        </p>
      </header>
      <div class="desktop-draft-grid">
        <aside class="desktop-card desktop-draft-lists">
          <Show
            when={!myRole()}
            fallback={
              <div class="desktop-draft-role">
                ASSIGNED · <strong>{roleLabel(role())}</strong>
              </div>
            }
          >
            <RoleSelector />
          </Show>
          <TierLists role={role()} />
        </aside>
        <section class="desktop-card desktop-draft-main">
          <div class="desktop-draft-left">
            <div class="desktop-draft-teams">
              <TeamRow
                label="MY TEAM"
                ids={cs()?.myTeamChampionIds ?? []}
                players={playerByChampion()}
              />
              <TeamRow
                label="ENEMY TEAM"
                ids={cs()?.enemyChampionIds ?? []}
                enemy
                players={playerByChampion()}
              />
            </div>
            <BansRow />
            <div class="border-t border-hx-border my-1" />
            <MatchupLine role={role()} />
            <Counters championId={vsEnemyId()} role={role()} onHoverChampion={setHoverChampId} />
            <Show when={active()}>
              <ImportButton
                championId={myChampionId()}
                role={role()}
                enemyId={vsEnemyId() || null}
              />
            </Show>
          </div>
          <div class={`desktop-draft-build ${hoverChampId() ? "is-previewing" : ""}`}>
            <div class="desktop-draft-build-header">
              <div class="hx-section-title">{hoverChampId() ? "RUNE PREVIEW" : "BUILD"}</div>
              <Show when={hoverChampId()}>
                {(id) => (
                  <span class="desktop-draft-preview-chip">
                    <Show when={assetsReady()}>
                      <Icon url={champIconByKey(id())} />
                    </Show>
                    <span>
                      <small>プレビュー中</small>
                      <strong>{champName(id()) || `#${id()}`}</strong>
                    </span>
                  </span>
                )}
              </Show>
            </div>
            <StatsRow championId={previewChamp()} role={role()} enemyId={previewEnemy()} />
            <div class="build-band">
              <BuildArea championId={previewChamp()} role={role()} enemyId={previewEnemy()} />
              <Show when={detailsValue()?.skillOrder}>
                <div class="build-extra-block">
                  <span class="build-extra-label">SKILL ORDER</span>
                  <SkillMatrix
                    order={detailsValue()?.skillOrder}
                    championImageId={champImageId(myChampionId())}
                  />
                </div>
              </Show>
            </div>
            <Show when={detailsValue()}>
              {(value) => (
                <Show when={value().skillOrder || value().items.length > 0}>
                  <div class="build-band build-band--sub">
                    <Show when={value().skillOrder}>
                      <div class="build-extra-block">
                        <span class="build-extra-label">SKILL MASTER</span>
                        <SkillMaster
                          order={value().skillOrder}
                          championImageId={champImageId(myChampionId())}
                        />
                      </div>
                    </Show>
                    <Show when={value().items.length > 0}>
                      <div class="build-extra-block">
                        <span class="build-extra-label">ITEM BUILD</span>
                        <ItemPath items={value().items} />
                      </div>
                    </Show>
                  </div>
                </Show>
              )}
            </Show>
          </div>
        </section>
      </div>
    </ScrollArea>
  );
}
