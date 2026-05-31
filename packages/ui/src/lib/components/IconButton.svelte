<script lang="ts">
  /**
   * Small icon-only button. Use for refresh / dismiss / inline toggle.
   * Pass the icon component via the `Icon` prop or use the default
   * children snippet to render a custom icon.
   *
   *   <IconButton Icon={RefreshCw} onclick={refresh} title="Refresh" />
   *   <IconButton onclick={dismiss} title="Dismiss">
   *     <X size={14} />
   *   </IconButton>
   */

  import type { Component, Snippet } from "svelte";

  interface Props {
    Icon?:     Component;
    size?:     number;
    onclick?:  (e: MouseEvent) => void;
    title?:    string;
    spinning?: boolean;
    tone?:     "muted" | "accent" | "secondary" | "error";
    children?: Snippet;
  }

  let {
    Icon,
    size = 12,
    onclick,
    title,
    spinning = false,
    tone = "muted",
    children,
  }: Props = $props();

  const COLOR: Record<NonNullable<Props["tone"]>, string> = {
    muted:     "var(--aim-text-muted)",
    accent:    "var(--aim-accent)",
    secondary: "var(--aim-secondary)",
    error:     "var(--aim-error)",
  };
</script>

<button {onclick} {title} class="s-1157ff1" style="--0c0: {COLOR[tone]};">
  {#if Icon}
    <Icon {size} class={spinning ? "animate-spin" : ""} />
  {:else if children}
    {@render children()}
  {/if}
</button>
