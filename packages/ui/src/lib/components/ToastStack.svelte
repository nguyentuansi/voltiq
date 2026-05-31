<script lang="ts">
  import { fly } from "svelte/transition";
  import { X } from "lucide-svelte";
  import { getToastContext } from "../contexts/toast.svelte";

  const toast = getToastContext();

  const TINT: Record<string, string> = {
    success: "var(--aim-accent)",
    error:   "var(--aim-danger)",
    info:    "var(--aim-accent-2)",
  };
</script>

<div class="right-4 bottom-4 flex z-50 fixed flex-col gap-2" aria-live="polite">
  {#each toast.items as t (t.id)}
    <div
      in:fly={{ duration: 200, y: 20 }}
      out:fly={{ duration: 150, x: 20 }}
      class="s-c5a4454 s-99ee518 s-84be8e4 items-center py-2 s-9670e5f s-f97f5aa s-e36394f s-a37611e s-c6d3a16 s-6d468ef s-cee0a2c s-e297a3f flex s-2ce50fc s-73d8f6a gap-3 s-9337491 s-1418f54 s-d267ef2 s-f198610 px-4 s-2301180" style="--fd1: 1px solid {TINT[t.kind]}; --444: {TINT[t.kind]};"

    >
      <span class="s-b5b8472">{t.text}</span>
      <button onclick={() => toast.dismiss(t.id)} class="s-2fe9211" aria-label="Dismiss">
        <X size={14} />
      </button>
    </div>
  {/each}
</div>
