import { For, Show } from "solid-js";
import { ROLES } from "../../lib/hexgate";
import { champSelect, selectedRole, setSelectedRole } from "../../state/backend";

export function RoleChips() {
  const cs = () => champSelect();
  const show = () => !cs()?.myRole && cs()?.active;

  return (
    <Show when={show()}>
      <div class="flex gap-1.5">
        <For each={ROLES}>
          {(r) => (
            <button
              type="button"
              class={`flex-1 font-hx-serif font-semibold text-[11px] tracking-[0.1em] py-[5px] rounded border cursor-pointer ${
                selectedRole() === r.lcu
                  ? "text-hx-gold border-hx-gold"
                  : "text-hx-muted border-hx-border bg-transparent"
              }`}
              onClick={() => {
                if (selectedRole() !== r.lcu) setSelectedRole(r.lcu);
              }}
            >
              {r.chip}
            </button>
          )}
        </For>
      </div>
    </Show>
  );
}
