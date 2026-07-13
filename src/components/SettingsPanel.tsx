import { invoke } from "@tauri-apps/api/core";
import { type JSX, Show } from "solid-js";
import {
  autoImport,
  autoOpenDraft,
  autoOpenLive,
  dataSource,
  dataSources,
  developerMode,
  importSpells,
  playerStatsSource,
  playerStatsSources,
  presentationMode,
  setAutoImport,
  setAutoOpenDraft,
  setAutoOpenLive,
  setDataSource,
  setDeveloperMode,
  setImportSpells,
  setPlayerStatsSource,
  setPresentationMode,
  setThemeMode,
  type ThemeMode,
  themeMode,
} from "../state/settings";
import type { PresentationMode } from "../types";
import { MobilePairing } from "./MobilePairing";

/** Display names for the backend `ProviderKind` ids (fallback: the raw id). */
const DATA_SOURCE_LABELS: Record<string, string> = {
  deeplol: "DeepLoL",
  ugg: "u.gg",
  lolalytics: "LoLalytics",
  opgg: "OP.GG",
};

function ToggleRow(props: {
  label: string;
  hint?: string;
  checked: boolean;
  onChange: (on: boolean) => void;
}) {
  return (
    <label class="flex items-center justify-between gap-3 cursor-pointer text-[12px] text-hx-text py-1">
      <span class="flex flex-col gap-0.5 min-w-0">
        <span>{props.label}</span>
        <Show when={props.hint}>
          <span class="text-[10px] text-hx-muted">{props.hint}</span>
        </Show>
      </span>
      <input
        type="checkbox"
        class="sr-only"
        checked={props.checked}
        onChange={(e) => props.onChange(e.currentTarget.checked)}
      />
      <span class="hx-switch" />
    </label>
  );
}

function Segmented<T extends string>(props: {
  label: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <div class="flex flex-col gap-1">
      <span class="text-[10px] tracking-[0.08em] text-hx-muted">{props.label}</span>
      <div
        class="grid gap-1 rounded border border-hx-border bg-hx-bg-raised p-1"
        style={{ "grid-template-columns": `repeat(${props.options.length}, 1fr)` }}
      >
        {props.options.map((option) => (
          <button
            type="button"
            class={`rounded px-2 py-1.5 font-hx-display text-[10px] font-bold tracking-[0.16em] cursor-pointer ${
              props.value === option.value
                ? "bg-hx-accent-wash text-hx-accent"
                : "bg-transparent text-hx-muted hover:text-hx-accent"
            }`}
            onClick={() => props.onChange(option.value)}
          >
            {option.label}
          </button>
        ))}
      </div>
    </div>
  );
}

function Section(props: { title: string; children: JSX.Element }) {
  return (
    <section class="flex flex-col gap-2">
      <div class="hx-section-title">{props.title}</div>
      {props.children}
    </section>
  );
}

export function SettingsForm() {
  const themeOptions: { value: ThemeMode; label: string }[] = [
    { value: "dark", label: "DARK" },
    { value: "light", label: "LIGHT" },
  ];
  const presentationOptions: { value: PresentationMode; label: string }[] = [
    { value: "overlay", label: "OVERLAY" },
    { value: "window", label: "WINDOW" },
  ];

  return (
    <div class="settings-form flex flex-col gap-5">
      <Section title="IMPORT">
        <ToggleRow
          label="ルーン自動インポート"
          hint="チャンプ確定時にルーンページを自動で書き込む"
          checked={autoImport()}
          onChange={(on) => {
            setAutoImport(on);
            invoke("set_auto_import", { enabled: on }).catch(() => {});
          }}
        />
        <ToggleRow
          label="サモナースペルも書き込む"
          checked={importSpells()}
          onChange={setImportSpells}
        />
      </Section>

      <Section title="DISPLAY">
        <Segmented
          label="表示モード"
          value={presentationMode()}
          options={presentationOptions}
          onChange={setPresentationMode}
        />
        <Segmented
          label="テーマ"
          value={themeMode()}
          options={themeOptions}
          onChange={setThemeMode}
        />
      </Section>

      <Section title="NAVIGATION">
        <ToggleRow
          label="チャンプセレクト開始時にドラフトを開く"
          checked={autoOpenDraft()}
          onChange={setAutoOpenDraft}
        />
        <ToggleRow
          label="試合開始時にLiveを開く"
          checked={autoOpenLive()}
          onChange={setAutoOpenLive}
        />
      </Section>

      <Show when={dataSources().length > 1}>
        <Section title="BUILD DATA">
          <label class="flex flex-col gap-1 text-hx-text">
            <span class="text-[10px] tracking-[0.08em] text-hx-muted">ビルドデータソース</span>
            <select
              class="bg-hx-panel border border-hx-border rounded px-2 py-1.5 text-[12px]"
              value={dataSource()}
              onChange={(e) => setDataSource(e.currentTarget.value)}
            >
              {dataSources().map((src) => (
                <option value={src}>{DATA_SOURCE_LABELS[src] ?? src}</option>
              ))}
            </select>
          </label>
        </Section>
      </Show>

      <Show when={playerStatsSources().length > 0}>
        <Section title="PLAYER STATS">
          <label class="flex flex-col gap-1 text-hx-text">
            <span class="text-[10px] tracking-[0.08em] text-hx-muted">戦績データソース</span>
            <select
              class="bg-hx-panel border border-hx-border rounded px-2 py-1.5 text-[12px]"
              value={playerStatsSource()}
              onChange={(event) => setPlayerStatsSource(event.currentTarget.value)}
            >
              {playerStatsSources().map((source) => (
                <option value={source.id}>{source.label}</option>
              ))}
            </select>
          </label>
        </Section>
      </Show>

      <Section title="MOBILE">
        <MobilePairing />
      </Section>

      <Section title="ADVANCED">
        <ToggleRow
          label="開発者モード"
          hint="デバッグパネルとモックシナリオを表示"
          checked={developerMode()}
          onChange={setDeveloperMode}
        />
      </Section>

      <div class="flex flex-col gap-1.5 pt-3 border-t border-hx-border text-[11px] text-hx-muted">
        <div class="flex items-center gap-2">
          <kbd class="hx-kbd">Ctrl+Shift+O</kbd>
          <span>このウィンドウを表示</span>
        </div>
        <div class="flex items-center gap-2">
          <kbd class="hx-kbd">Ctrl+Shift+M</kbd>
          <span>オーバーレイを次のモニターへ移動</span>
        </div>
      </div>
    </div>
  );
}
