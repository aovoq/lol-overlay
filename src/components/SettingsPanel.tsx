import { createEffect, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { interactive } from "../state/backend";
import {
  autoImport,
  dataSource,
  dataSources,
  gearHidden,
  importSpells,
  setAutoImport,
  setDataSource,
  setGearHidden,
  setImportSpells,
} from "../state/settings";

export function SettingsPanel() {
  createEffect(() => {
    const on = interactive();
    document.body.classList.toggle("interactive", on);
    if (!on) setGearHidden(true);
  });

  const open = () => interactive() || !gearHidden();

  return (
    <Show when={open()}>
      <div
        class="panel settings-panel fixed bottom-4 right-4 w-[220px] flex flex-col gap-2 pointer-events-auto"
        data-hit
      >
        <div class="font-hx-serif text-[11px] font-bold tracking-[0.28em] text-hx-gold">
          lol-overlay
        </div>
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
          Ctrl+Shift+O 全体操作(非常用) · Ctrl+Shift+M モニター移動
        </div>
      </div>
    </Show>
  );
}
