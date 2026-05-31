<script lang="ts">
  /**
   * Standard list-page chrome shared by /findings, /http-records, /scans,
   * /extensions, /agentic-scan, /oast-interactions:
   *
   *   bordered frame
   *     toolbar  (icon + title + refresh + extras-left | extras-right)
   *     body
   *     pagination footer  (when total + offset/pageSize are wired)
   *
   * Pagination is computed internally; pass `total` + `bind:offset` + (optional)
   * `pageSize`. Skip them for routes that don't paginate.
   *
   *   <ListPage
   *     title="FINDINGS"
   *     titleIcon={Shield}
   *     isFetching={q.isFetching}
   *     onRefresh={() => q.refetch()}
   *     total={q.data?.total}
   *     bind:offset
   *   >
   *     {#snippet toolbarRight()} … filter chips and search … {/snippet}
   *     {#snippet body()}        … StickyTable goes here    … {/snippet}
   *   </ListPage>
   */

  import type { Component, Snippet } from "svelte";
  import { RefreshCw } from "lucide-svelte";

  interface Props {
    title:        string;
    titleIcon?:   Component;
    titleColor?:  string;
    onRefresh?:   () => void;
    isFetching?:  boolean;

    // Pagination — provide `total` AND `offset` to enable the footer.
    total?:       number;
    offset?:      number;
    pageSize?:    number;

    toolbarLeft?:  Snippet;
    toolbarRight?: Snippet;
    body:          Snippet;
  }

  let {
    title,
    titleIcon: TitleIcon,
    titleColor = "var(--aim-accent)",
    onRefresh,
    isFetching = false,
    total,
    offset = $bindable(0),
    pageSize = 100,
    toolbarLeft,
    toolbarRight,
    body,
  }: Props = $props();

  const paginated = $derived(total !== undefined && total > 0);
  const totalPages  = $derived(Math.max(1, Math.ceil((total ?? 0) / pageSize)));
  const currentPage = $derived(Math.floor(offset / pageSize) + 1);
  const pageEnd     = $derived(Math.min(offset + pageSize, total ?? 0));
</script>

<div class="flex-1 flex min-h-[500px]">
  <div
    class="overflow-hidden flex flex-col w-full s-b0c54ba"
   
  >
    <!-- Toolbar -->
    <div
      class="flex-wrap items-center s-8aaa438 flex py-1.5 px-3 gap-2 justify-between"
     
    >
      <div class="items-center gap-1.5 flex">
        {#if TitleIcon}<TitleIcon size={12} class="s-5b6d912" style="--4c4: {titleColor};" />{/if}
        <span class="text-xs s-5b6d912 font-bold" style="--4c4: {titleColor};">{title}</span>
        {#if onRefresh}
          <button onclick={onRefresh} title="Refresh" class="s-f905530">
            <RefreshCw size={12} class={isFetching ? "animate-spin" : ""} />
          </button>
        {/if}
        {#if toolbarLeft}{@render toolbarLeft()}{/if}
      </div>

      {#if toolbarRight}
        <div class="flex-wrap items-center text-xs flex gap-2">
          {@render toolbarRight()}
        </div>
      {/if}
    </div>

    <!-- Body -->
    <div class="flex-1 overflow-auto">
      {@render body()}
    </div>

    <!-- Pagination footer (only when total > 0) -->
    {#if paginated}
      <div
        class="items-center text-xs s-7bafd5a py-1 flex px-3 justify-between"
       
      >
        <span>{offset + 1}-{pageEnd}/{total}</span>
        <div class="items-center flex gap-1">
          <button
            onclick={() => (offset = Math.max(0, offset - pageSize))}
            disabled={currentPage <= 1}
            class="px-1"
          >&lt;</button>
          <span class="px-1">{currentPage}/{totalPages}</span>
          <button
            onclick={() => { if (currentPage < totalPages) offset += pageSize; }}
            disabled={currentPage >= totalPages}
            class="px-1"
          >&gt;</button>
        </div>
      </div>
    {/if}
  </div>
</div>
