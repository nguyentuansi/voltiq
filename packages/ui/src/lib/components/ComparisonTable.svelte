<script lang="ts" generics="R">
  /**
   * Feature × column comparison table. The leftmost column is the
   * feature label (from `featureKey`), subsequent columns come from
   * `columns`. The right-most column gets the highlighted accent
   * background.
   *
   *   <ComparisonTable
   *     columns={[
   *       { key: "native",  label: "Native Scan" },
   *       { key: "agentic", label: "Agentic Scan" },
   *     ]}
   *     rows={scanModes.rows}
   *     featureKey="label"
   *     highlightLast
   *   />
   *
   *   <!-- For more arbitrary cell rendering, pass the `cell` snippet -->
   *   <ComparisonTable {columns} {rows} featureKey="feature">
   *     {#snippet cell(row, col)}
   *       {#if typeof row[col.key] === "boolean"}
   *         <Check size={14} />
   *       {:else}
   *         {row[col.key]}
   *       {/if}
   *     {/snippet}
   *   </ComparisonTable>
   */

  import type { Snippet } from "svelte";

  export interface ComparisonColumn {
    key:   string;
    label: string;
  }

  interface Props<RowT> {
    columns:        ComparisonColumn[];
    rows:           RowT[];
    /** Property on each row that holds the row label. */
    featureKey:     keyof RowT;
    /** Highlights the last column with a soft accent background. */
    highlightLast?: boolean;
    /** Optional custom cell renderer (otherwise prints `row[col.key]`). */
    cell?:          Snippet<[RowT, ComparisonColumn]>;
  }

  let { columns, rows, featureKey, highlightLast = false, cell }: Props<R> = $props();

  const lastIdx = $derived(columns.length - 1);
</script>

<div
  class="overflow-x-auto s-8942b7b"
 
>
  <table class="s-d1dfc6a">
    <thead>
      <tr class="s-b0342e3">
        <th
          class="s-566be94 uppercase text-left font-bold"
         
        ></th>
        {#each columns as c, i (c.key)}
          <th
            class="text-center uppercase s-f16bd21 font-bold" style="--904: {highlightLast && i === lastIdx ? '#50fa7b' : 'var(--aim-text-muted)'}; --e0e: {highlightLast && i === lastIdx ? 'rgba(80,250,123,0.05)' : 'transparent'};"
           
          >{c.label}</th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each rows as row, rowIdx (rowIdx)}
        <tr class="s-b32a745" style="--919: {rowIdx === rows.length - 1 ? 'none' : '1px solid rgba(252,232,195,0.05)'};">
          <td
            class="s-302f25a uppercase font-bold"
           
          >{row[featureKey]}</td>
          {#each columns as col, i (col.key)}
            <td
              class="text-center s-2fccdba" style="--e0e: {highlightLast && i === lastIdx ? 'rgba(80,250,123,0.04)' : 'transparent'};"
             
            >
              {#if cell}
                {@render cell(row, col)}
              {:else}
                {row[col.key as keyof R]}
              {/if}
            </td>
          {/each}
        </tr>
      {/each}
    </tbody>
  </table>
</div>
