import { invoke } from "@tauri-apps/api/core";
import { type JSX, Show } from "solid-js";
import { dataSourceLabel } from "../lib/openlol";
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

function ToggleRow(props: {
  label: string;
  hint?: string;
  checked: boolean;
  onChange: (on: boolean) => void;
}) {
  return (
    <label class="settings-toggle">
      <span class="settings-toggle-copy">
        <strong>{props.label}</strong>
        <Show when={props.hint}>
          <small>{props.hint}</small>
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
    <div class="settings-segmented">
      <span>{props.label}</span>
      <div
        class="settings-segmented-options"
        style={{ "grid-template-columns": `repeat(${props.options.length}, 1fr)` }}
      >
        {props.options.map((option) => (
          <button
            type="button"
            class={props.value === option.value ? "is-active" : ""}
            onClick={() => props.onChange(option.value)}
          >
            {option.label}
          </button>
        ))}
      </div>
    </div>
  );
}

function Section(props: {
  title: string;
  meta: string;
  description: string;
  wide?: boolean;
  children: JSX.Element;
}) {
  return (
    <section class={`settings-section ${props.wide ? "settings-section--wide" : ""}`}>
      <header class="settings-section-header">
        <span>{props.meta}</span>
        <h2>{props.title}</h2>
        <p>{props.description}</p>
      </header>
      <div class="settings-section-body">{props.children}</div>
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
    <div class="settings-form">
      <div class="settings-grid">
        <Section
          meta="IMPORT"
          title="自動インポート"
          description="チャンピオン確定後にクライアントへ反映します。"
        >
          <ToggleRow
            label="ルーンを自動で書き込む"
            hint="確定したチャンピオンとロールに合うページを作成"
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

        <Section
          meta="DISPLAY"
          title="画面表示"
          description="プレイ環境に合わせて表示方法を選びます。"
        >
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

        <Section
          meta="NAVIGATION"
          title="自動画面切り替え"
          description="ゲームの進行に合わせて必要な画面を開きます。"
        >
          <ToggleRow
            label="ドラフト開始時に開く"
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
          <Section
            meta="BUILD DATA"
            title="ビルドデータ"
            description="推薦ビルドに使用する提供元を選択します。"
          >
            <label class="settings-select-field">
              <span>データ提供元</span>
              <select value={dataSource()} onChange={(e) => setDataSource(e.currentTarget.value)}>
                {dataSources().map((src) => (
                  <option value={src}>{dataSourceLabel(src)}</option>
                ))}
              </select>
            </label>
          </Section>
        </Show>

        <Show when={playerStatsSources().length > 0}>
          <Section
            meta="PLAYER STATS"
            title="戦績データ"
            description="サモナー検索に使用する提供元を選択します。"
          >
            <label class="settings-select-field">
              <span>データ提供元</span>
              <select
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

        <Section
          meta="MOBILE"
          title="iPhoneサイドボード"
          description="試合中の情報を手元の端末にも表示します。"
          wide
        >
          <MobilePairing />
        </Section>

        <Section
          meta="ADVANCED"
          title="開発者向け"
          description="モックシナリオとイベントログを有効にします。"
          wide
        >
          <ToggleRow
            label="開発者モード"
            hint="デバッグ用の操作とログを表示"
            checked={developerMode()}
            onChange={setDeveloperMode}
          />
        </Section>
      </div>

      <div class="settings-shortcuts">
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
