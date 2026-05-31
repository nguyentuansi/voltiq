<script lang="ts" generics="T">
  /**
   * Generic sticky-header table. Header is built from a `columns` prop;
   * each row is rendered via the `row` snippet which receives the item
   * and its index. Empty state is one row spanning all columns.
   *
   *   <StickyTable {columns} items={findings} empty="no findings">
   *     {#snippet row(f)}
   *       <tr class="hover:bg-[…]">
   *         <td class="py-1 px-2">{f.severity}</td>
   *         …
   *       </tr>
   *     {/snippet}
   *   </StickyTable>
   *
   * Keep `<tr>` inside the snippet so the consumer controls hover styling,
   * key attribute, click handlers, and per-row state.
   */

  import type { Snippet } from "svelte";

  export interface Column {
    key:    string;
    label:  string;
    align?: "left" | "right";
    width?: string;     // any CSS width value, applied as inline style
  }

  interface Props {
    columns: Column[];
    items:   T[];
    empty?:  string;
    row:     Snippet<[T, number]>;
  }

  let { columns, items, empty = "no data", row }: Props = $props();
</script>

<table class="text-xs s-6cf831c w-full">
  <thead>
    <tr class="sticky z-10 top-0 s-f5fc12a">
      {#each columns as c (c.key)}
        <th
          class="px-2 py-1 whitespace-nowrap {c.align === 'right' ? 'text-right' : 'text-left'}"
          style="border-bottom: 1px solid var(--aim-border); color: var(--aim-text-muted); {c.width ? `width: ${c.width};` : ''}"
        >{c.label}</th>
      {/each}
    </tr>
  </thead>
  <tbody>
    {#each items as item, i (i)}
      {@render row(item, i)}
    {/each}
    {#if items.length === 0}
      <tr>
        <td colspan={columns.length} class="text-center s-f905530 px-3 py-4">
          {empty}
        </td>
      </tr>
    {/if}
  </tbody>
</table>
