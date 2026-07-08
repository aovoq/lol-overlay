import { invoke } from "@tauri-apps/api/core";
import { createMemo, Show } from "solid-js";
import {
  activeTab,
  champSelect,
  importState,
  selectedRole,
  setImportState,
  vsEnemyId,
} from "../../state/backend";
import { importSpells, spellsFlipped } from "../../state/settings";

function effectiveRole() {
  const cs = champSelect();
  return cs?.myRole || selectedRole();
}

export function ImportButton() {
  const my = createMemo(() => champSelect()?.myChampionId ?? 0);
  let importTimer: number | undefined;

  const finishImport = (state: "imported" | "failed", revertAfterMs: number) => {
    setImportState(state);
    importTimer = window.setTimeout(() => setImportState("idle"), revertAfterMs);
  };

  const label = createMemo(() => {
    switch (importState()) {
      case "importing":
        return "IMPORTING…";
      case "imported":
        return "IMPORTED ✓";
      case "failed":
        return "FAILED — RETRY";
      default:
        return importSpells() ? "IMPORT RUNES & SPELLS" : "IMPORT RUNES";
    }
  });

  const onClick = () => {
    const champ = my();
    if (!champ || importState() === "importing") return;
    if (importTimer) window.clearTimeout(importTimer);
    setImportState("importing");
    invoke("import_build", {
      championId: champ,
      role: effectiveRole(),
      enemyChampionId: activeTab() === "vs" && vsEnemyId() ? vsEnemyId() : null,
      includeSpells: importSpells(),
      flipSpells: spellsFlipped(),
    }).then(
      () => finishImport("imported", 2000),
      () => finishImport("failed", 3000),
    );
  };

  return (
    <Show when={my()}>
      <button
        type="button"
        class={`hx-primary-button w-full h-11 rounded-md font-hx-display font-extrabold text-[13px] tracking-[0.18em] cursor-pointer disabled:opacity-65 disabled:cursor-default ${
          importState() === "failed" ? "bg-hx-red" : ""
        }`}
        disabled={importState() === "importing"}
        onClick={onClick}
      >
        {label()}
      </button>
      <div class="flex justify-center py-2 pb-0.5">
        <span class="border border-hx-accent-dim rounded-[3px] px-3 py-0.5 font-hx-display font-semibold text-[10px] tracking-[0.22em] text-hx-accent-dim">
          PRO
        </span>
      </div>
    </Show>
  );
}
