import { invoke } from "@tauri-apps/api/core";
import { Show } from "solid-js";
import {
  autoImport,
  dataSource,
  dataSources,
  importSpells,
  presentationMode,
  setAutoImport,
  setDataSource,
  setImportSpells,
  setPresentationMode,
  setThemeMode,
  type ThemeMode,
  themeMode,
} from "../state/settings";
import type { PresentationMode } from "../types";

export function SettingsForm() {
  const themeOptions: { value: ThemeMode; label: string }[] = [
    { value: "dark", label: "Dark" },
    { value: "light", label: "Light" },
  ];
  const presentationOptions: { value: PresentationMode; label: string }[] = [
    { value: "overlay", label: "Overlay" },
    { value: "window", label: "Window" },
  ];

  return (
    <div class="settings-form flex flex-col gap-2">
      <div class="font-hx-serif text-[11px] font-bold tracking-[0.28em] text-hx-gold">SETTINGS</div>
      <label class="flex items-center gap-2 cursor-pointer text-hx-text">
        <input
          type="checkbox"
          checked={autoImport()}
          onChange={(e) => {
            setAutoImport(e.currentTarget.checked);
            invoke("set_auto_import", {
              enabled: e.currentTarget.checked,
            }).catch(() => {});
          }}
          class="accent-hx-gold"
        />
        <span>ルーン自動インポート</span>
      </label>
      <label class="flex items-center gap-2 cursor-pointer text-hx-text">
        <input
          type="checkbox"
          checked={importSpells()}
          onChange={(e) => setImportSpells(e.currentTarget.checked)}
          class="accent-hx-gold"
        />
        <span>スペルも書き込む</span>
      </label>
      <div class="flex flex-col gap-1">
        <span class="text-[11px] text-hx-muted">表示モード</span>
        <div class="grid grid-cols-2 gap-1 rounded border border-hx-border bg-hx-bg-raised p-1">
          {presentationOptions.map((option) => (
            <button
              type="button"
              class={`rounded px-2 py-1 font-hx-serif text-[10px] font-semibold tracking-[0.16em] cursor-pointer ${
                presentationMode() === option.value
                  ? "bg-hx-gold-wash text-hx-gold"
                  : "bg-transparent text-hx-muted hover:text-hx-gold"
              }`}
              onClick={() => setPresentationMode(option.value)}
            >
              {option.label}
            </button>
          ))}
        </div>
      </div>
      <div class="flex flex-col gap-1">
        <span class="text-[11px] text-hx-muted">テーマ</span>
        <div class="grid grid-cols-2 gap-1 rounded border border-hx-border bg-hx-bg-raised p-1">
          {themeOptions.map((option) => (
            <button
              type="button"
              class={`rounded px-2 py-1 font-hx-serif text-[10px] font-semibold tracking-[0.16em] cursor-pointer ${
                themeMode() === option.value
                  ? "bg-hx-gold-wash text-hx-gold"
                  : "bg-transparent text-hx-muted hover:text-hx-gold"
              }`}
              onClick={() => setThemeMode(option.value)}
            >
              {option.label}
            </button>
          ))}
        </div>
      </div>
      <Show when={dataSources().length > 1}>
        <label class="flex flex-col gap-1 text-hx-text">
          <span class="text-[11px] text-hx-muted">データソース</span>
          <select
            class="bg-hx-panel border border-hx-border rounded px-2 py-1 text-[12px]"
            value={dataSource()}
            onChange={(e) => setDataSource(e.currentTarget.value)}
          >
            {dataSources().map((src) => (
              <option value={src}>{src}</option>
            ))}
          </select>
        </label>
      </Show>
      <div class="text-[11px] text-hx-muted">
        Ctrl+Shift+O でこのウィンドウを表示 · Ctrl+Shift+M でオーバーレイ移動
      </div>
    </div>
  );
}
