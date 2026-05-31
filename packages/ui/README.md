# `@landing-v/ui` — shared design system

Terminal-aesthetic Svelte 5 components shared by every workspace in
`landing-v`. Both `apps/web` (landing) and `apps/console` (operations
dashboard) consume from here:

```ts
import {
  // page shells & primitives
  Section, ListPage, StickyTable, type Column,
  // content blocks
  Card, CardGrid, SectionHeading, CornerBrackets, RegistrationMark,
  Stat, AccordionItem, ComparisonTable, type ComparisonColumn,
  // form controls
  SearchBox, FilterChips, Select,
  // buttons & links
  BracketButton, IconButton, AccentLink,
  // feedback
  ToastStack, setToastContext, getToastContext, type Toast,
  // motion
  inView, textScramble, scrambleCounter,
  // theme tokens
  STATUS_TINT, SEVERITY_COLORS, SEVERITY_ORDER, CONFIDENCE_COLORS,
  METHOD_COLORS, statusColor,
  // formatters
  fmtTime, fmtDur,
} from "@landing-v/ui";
```

The package is a **source-export Svelte package** — no build step. Vite
resolves the `.svelte` files directly via the `svelte` package.json
condition. Hot-module-reload works the same as in-app components.

## Architecture

- **No tree shaking required.** Everything is one re-export tree from
  `src/index.ts`. Add your component there if it should be public.
- **Svelte 5 runes (`$state`, `$derived`, `$props`, `$bindable`).** Don't
  mix with stores or Svelte 4 component shapes.
- **Theme via CSS custom properties.** Every component reads from
  `var(--aim-*)` declared in each app's `app.css`. To re-theme an entire
  app, override those vars in your `:root`/`[data-theme]` rule. **Do
  not** hardcode hex values inside components (except the `#50fa7b`
  green accent for `CornerBrackets` — that's intentionally fixed).
- **Tailwind 4 utilities** are available app-side; the package itself
  inlines styles where the look is non-trivial (borders, layout) and
  relies on Tailwind for spacing / sizing. The `scripts/hash-inline-
  styles.mjs` build step folds those inline styles into hashed classes
  at compile time so the rendered DOM doesn't fingerprint-match the
  upstream Vigolium site.

## Component reference

### Layout & chrome

#### `<Section>` — landing-page section wrapper

```svelte
<Section id="how-it-works" maxWidth="max-w-6xl">
  <SectionHeading annotation="Workflow" heading="How It Works" />
  <CardGrid …>…</CardGrid>
</Section>
```

| prop       | type   | default        | description                                |
|------------|--------|----------------|--------------------------------------------|
| `id`       | string | —              | scroll-anchor id (`#pricing`, `#faq`, …)   |
| `maxWidth` | string | `"max-w-6xl"`  | Tailwind max-width for the inner column    |
| `class`    | string | —              | extra utility classes on the `<section>`   |
| `children` | snippet | —             | section body                               |

Adds the `aim-section` class which provides scroll-margin + positioning.

---

#### `<ListPage>` — toolbar + body + pagination

The chrome every console list page sits inside.

```svelte
<ListPage
  title="FINDINGS"
  titleIcon={Shield}
  titleColor="var(--aim-accent)"
  isFetching={q.isFetching}
  onRefresh={() => q.refetch()}
  total={q.data?.total}
  bind:offset
  pageSize={100}
>
  {#snippet toolbarLeft()}…filter chips…{/snippet}
  {#snippet toolbarRight()}…search + dropdowns…{/snippet}
  {#snippet body()}<StickyTable …>…</StickyTable>{/snippet}
</ListPage>
```

| prop           | type            | default              | description                                            |
|----------------|-----------------|----------------------|--------------------------------------------------------|
| `title`        | string          | —                    | bold uppercase label                                   |
| `titleIcon`    | lucide Component | —                  | optional icon before the title                         |
| `titleColor`   | string          | `var(--aim-accent)`  | title + icon colour                                    |
| `onRefresh`    | function        | —                    | when provided, shows a refresh button next to the title |
| `isFetching`   | boolean         | `false`              | spins the refresh icon                                 |
| `total`        | number          | —                    | total result count (drives pagination)                 |
| `offset`       | number          | `0`                  | **bindable** — current pagination offset               |
| `pageSize`     | number          | `100`                | rows per page                                          |
| `toolbarLeft`  | snippet         | —                    | extras after the refresh button                        |
| `toolbarRight` | snippet         | —                    | search / dropdowns on the right                        |
| `body`         | snippet         | —                    | main content area                                      |

Pagination footer is shown automatically when `total > 0`.

---

#### `<StickyTable>` — sticky-header HTML table

```svelte
<StickyTable {columns} items={findings} empty="no findings">
  {#snippet row(f, i)}
    <tr style="border-bottom: 1px solid var(--aim-border);">
      <td class="px-2 py-1">{f.severity}</td>
      …
    </tr>
  {/snippet}
</StickyTable>
```

| prop      | type           | default     | description                                  |
|-----------|----------------|-------------|----------------------------------------------|
| `columns` | `Column[]`     | —           | header definitions                           |
| `items`   | `T[]`          | —           | row data (generic in `T`)                    |
| `empty`   | string         | `"no data"` | text shown when `items.length === 0`         |
| `row`     | `Snippet<[T, number]>` | —   | renders each `<tr>` (you own the markup)     |

```ts
export interface Column {
  key:    string;
  label:  string;
  align?: "left" | "right";
  width?: string;   // any CSS width value
}
```

The component renders `<table>` + `<thead>` + `<tbody>` and delegates
each row's `<tr>` to your snippet. Keep `<tr>` inside the snippet so you
control hover, key, and click handlers.

---

### Content blocks

#### `<Card>` — bordered translucent panel

```svelte
<Card tilt padding="p-8" class="h-full">
  <CornerBrackets />
  <h3 class="aim-heading">{step.title}</h3>
  <p class="aim-body">{step.description}</p>
</Card>
```

| prop      | type    | default | description                                              |
|-----------|---------|---------|----------------------------------------------------------|
| `tilt`    | boolean | `false` | adds 3D mousemove tilt + hover-glow                      |
| `padding` | string  | `"p-6"` | Tailwind padding class                                   |
| `class`   | string  | —       | extra utilities (e.g. `h-full`)                          |
| `style`   | string  | —       | inline style passthrough                                 |
| `children` | snippet | —      | card body                                                |

---

#### `<CardGrid>` — responsive grid with reveal-on-scroll stagger

```svelte
<CardGrid items={howItWorks} cols={3} stagger={150}>
  {#snippet item(step, i)}
    <Card tilt>…</Card>
  {/snippet}
</CardGrid>
```

| prop      | type           | default | description                                    |
|-----------|----------------|---------|------------------------------------------------|
| `items`   | `T[]`          | —       | data array (generic in `T`)                    |
| `cols`    | `1 \| 2 \| 3 \| 4` | `3` | columns at `lg:` breakpoint; steps down on smaller |
| `stagger` | number         | `100`   | per-item delay in ms                           |
| `gap`     | string         | `"gap-2"` | Tailwind gap class                            |
| `item`    | `Snippet<[T, number]>` | — | renders each card                          |

Each item is wrapped in an `inView` action; opacity/translate fade in
as the row enters the viewport.

---

#### `<SectionHeading>` — `// annotation` + scrambling heading

```svelte
<SectionHeading annotation="Common questions" heading="FAQ" />
```

| prop         | type   | description                |
|--------------|--------|----------------------------|
| `annotation` | string | small kicker above heading |
| `heading`    | string | bold uppercase H2          |

Uses `textScramble` so the heading hacker-scrambles into view.

---

#### `<CornerBrackets>` — 4 L-shaped corner marks

```svelte
<Card>
  <CornerBrackets />
  …
</Card>
```

| prop   | type   | default     | description                |
|--------|--------|-------------|----------------------------|
| `size` | string | `"w-7 h-7"` | Tailwind w/h on each mark  |

Renders 4 absolutely-positioned `<span>` elements at the corners of its
nearest `relative` ancestor. Always green (`#50fa7b`) — the look is
intentionally fixed.

---

#### `<RegistrationMark>` — single `+` register mark

```svelte
<RegistrationMark className="-top-1 -right-2" />
```

| prop        | type   | default | description                                  |
|-------------|--------|---------|----------------------------------------------|
| `className` | string | `""`    | absolute positioning utilities (e.g. `-top-2 -left-2`) |

Tiny green `+` glyph for marking corners or focal points. Caller is
responsible for the `absolute …` positioning classes via `className`.

---

#### `<Stat>` — big-number stat tile

```svelte
<Stat value="2,400+" label="findings shipped" accent="#50fa7b" />

<!-- With digit-rolling scramble on appear -->
<Stat value="$1.2M" label="raised" accent="#53bdfa" scramble />
```

| prop      | type   | default     | description                                       |
|-----------|--------|-------------|---------------------------------------------------|
| `value`   | string | —           | the big number / displayed value                  |
| `label`   | string | —           | small uppercase label below                       |
| `accent`  | string | `"#fce8c3"` | hex/CSS colour for the number + hover glow + border |
| `scramble`| boolean | `false`    | applies `scrambleCounter` on viewport entry       |

Pre-styled bordered panel with `CornerBrackets` and an accent-coloured
hover glow. Sized for 3-up grids.

---

#### `<AccentLink>` — coloured CTA anchor

```svelte
<AccentLink href="/sign-up" accent="#50fa7b">
  Sign up <ArrowRight size={14} />
</AccentLink>

<!-- Filled variant: bg fills with accent on hover -->
<AccentLink href="/pro" accent={tier.color} variant="fill">
  Choose plan
</AccentLink>
```

| prop     | type                  | default                | description                              |
|----------|-----------------------|------------------------|------------------------------------------|
| `href`   | string                | —                      | required, renders `<a>`                  |
| `accent` | string                | `var(--aim-accent)`    | hex/var; drives border + text colour + hover glow |
| `variant`| `"border"\|"fill"`    | `"border"`             | `fill` inverts to `accent` background on hover |
| `target` / `rel` / `title` | string  | —                      | passed through                           |
| `padding`| string                | `"py-3 px-5"`          | Tailwind padding                         |
| `class`  | string                | —                      | extra utility classes (e.g. `justify-center`) |
| `children`| snippet              | —                      | label / icons / arrows                   |

Hover behaviour:
- `border` (default): bg fills 8 % accent, border deepens, soft glow.
- `fill`: bg becomes solid accent, text becomes dark bg colour.

---

#### `<AccordionItem>` — bordered click-to-expand row

```svelte
<!-- Solo -->
<AccordionItem question="What is …?" bind:open>
  {#snippet body()}<p class="aim-body">…</p>{/snippet}
</AccordionItem>

<!-- Exclusive group -->
{#each items as it, i (it.q)}
  <AccordionItem
    question={it.q}
    open={selected === i}
    onToggle={() => (selected = selected === i ? null : i)}
  >
    {#snippet body()}<p class="aim-body">{it.a}</p>{/snippet}
  </AccordionItem>
{/each}
```

| prop       | type                | default              | description                              |
|------------|---------------------|----------------------|------------------------------------------|
| `question` | string              | —                    | the row label                            |
| `open`     | boolean             | `false`              | **bindable**; controls expanded state    |
| `onToggle` | `() => void`        | —                    | overrides internal toggle (use for exclusive groups) |
| `accent`   | string              | `var(--aim-accent)`  | colour for active border + icon          |
| `body`     | snippet             | —                    | content shown when open                  |

`+`/`−` icon flips automatically. Slide-in animation on expand.

---

#### `<ComparisonTable>` — feature × column comparison table

```svelte
<ComparisonTable
  columns={[
    { key: "native",  label: "Native Scan"  },
    { key: "agentic", label: "Agentic Scan" },
  ]}
  rows={scanModes.rows}
  featureKey="label"
  highlightLast
/>

<!-- Custom cell renderer for booleans -->
<ComparisonTable {columns} {rows} featureKey="feature" highlightLast>
  {#snippet cell(row, col)}
    {#if typeof row[col.key] === "boolean"}
      <Check size={14} />
    {:else}
      {row[col.key]}
    {/if}
  {/snippet}
</ComparisonTable>
```

| prop            | type                              | default | description                                       |
|-----------------|-----------------------------------|---------|---------------------------------------------------|
| `columns`       | `ComparisonColumn[]`              | —       | header definitions                                |
| `rows`          | `R[]`                             | —       | data rows (generic in `R`)                        |
| `featureKey`    | `keyof R`                         | —       | property on each row used for the leftmost column |
| `highlightLast` | boolean                           | `false` | gives the last column an accent-tinted background |
| `cell`          | `Snippet<[R, ComparisonColumn]>`  | —       | optional custom renderer; falls back to `row[col.key]` |

```ts
export interface ComparisonColumn {
  key:   string;
  label: string;
}
```

Renders a translucent bordered table with sticky-header styling and
optional last-column highlight (for "this is the option we'd pick" rows).

---

### Form controls

#### `<SearchBox>` — icon + text input combo

```svelte
<SearchBox bind:value={search} placeholder="search..." />
<SearchBox bind:value={domainFilter} Icon={Globe}
           placeholder="domain..." width="w-28" />
```

| prop          | type     | default        | description                          |
|---------------|----------|----------------|--------------------------------------|
| `value`       | string   | `""`           | **bindable** input value             |
| `placeholder` | string   | `"search..."`  |                                      |
| `Icon`        | lucide Component | `Search` | leading icon                       |
| `width`       | string   | `"w-36"`       | Tailwind width on the `<input>`      |
| `oninput`     | function | —              | fires every keystroke                |

Used heavily in console toolbars (search, domain, module, host, source).

---

#### `<FilterChips>` — toggleable bracket-row

```svelte
<FilterChips
  bind:value={statusFilter}
  tone="accent"
  options={[
    { v: "",          label: "all" },
    { v: "running",   label: "running" },
    { v: "completed", label: "completed" },
  ]}
/>
```

| prop       | type                 | default    | description                              |
|------------|----------------------|------------|------------------------------------------|
| `value`    | string               | —          | **bindable** currently-selected key      |
| `options`  | `{ v, label }[]`     | —          | available chips                          |
| `tone`     | `"accent"\|"secondary"\|"tertiary"` | `"accent"` | highlight colour for the active chip |
| `onchange` | `(v: string) => void` | —         | fires after selection                    |

Use for status/severity/method filter rows.

---

#### `<Select>` — styled `<select>` dropdown

```svelte
<Select bind:value={modeFilter}>
  <option value="">mode:all</option>
  <option>autopilot</option>
  <option>swarm</option>
</Select>
```

| prop      | type             | description                          |
|-----------|------------------|--------------------------------------|
| `value`   | string           | **bindable** selection               |
| `onchange`| `(e) => void`    | fires on change                      |
| `children`| snippet (options) | pass `<option>` elements as children |

---

### Buttons

#### `<BracketButton>` — terminal-style `[label]`

```svelte
<BracketButton tone="error" onclick={stop}>[stop]</BracketButton>
<BracketButton tone="accent" variant="boxed" href="/scan/new">
  [+ new scan]
</BracketButton>
```

| prop       | type                                 | default     | description                              |
|------------|--------------------------------------|-------------|------------------------------------------|
| `tone`     | `"accent"\|"secondary"\|"tertiary"\|"success"\|"error"\|"muted"\|"text"` | `"muted"` | colour from theme palette |
| `variant`  | `"inline"\|"boxed"`                  | `"inline"`  | `boxed` adds a 1px coloured border       |
| `href`     | string                               | —           | render `<a>` instead of `<button>`       |
| `onclick`  | `(e) => void`                        | —           |                                          |
| `disabled` | boolean                              | `false`     | only meaningful on `<button>`            |
| `children` | snippet                              | —           | include the `[`/`]` literally inside     |

The brackets `[…]` are literal text — you control whether they show.
The component just colours and styles the children.

---

#### `<IconButton>` — small icon-only button

```svelte
<IconButton Icon={RefreshCw} onclick={refresh} title="Refresh"
            spinning={isFetching} />
<IconButton onclick={dismiss} title="Dismiss" tone="error">
  <X size={14} />
</IconButton>
```

| prop      | type                                 | default | description                              |
|-----------|--------------------------------------|---------|------------------------------------------|
| `Icon`    | lucide Component                     | —       | icon to render (or use `children`)       |
| `size`    | number                               | `12`    | icon pixel size                          |
| `onclick` | `(e) => void`                        | —       |                                          |
| `title`   | string                               | —       | tooltip                                  |
| `spinning`| boolean                              | `false` | adds `animate-spin`                      |
| `tone`    | `"muted"\|"accent"\|"secondary"\|"error"` | `"muted"` | colour                            |
| `children`| snippet                              | —       | custom icon when `Icon` not used         |

---

### Feedback

#### `<ToastStack>` + `setToastContext` / `getToastContext`

```svelte
<!-- In root +layout.svelte: -->
<script>
  import { ToastStack, setToastContext } from "@landing-v/ui";
  setToastContext();
</script>
…
<ToastStack />
```

```svelte
<!-- Anywhere downstream: -->
<script>
  import { getToastContext } from "@landing-v/ui";
  const toast = getToastContext();
</script>
<button onclick={() => toast.push({ kind: "success", text: "Scan started" })}>
  Start scan
</button>
```

```ts
interface Toast        { id: number; kind: "success"|"error"|"info"; text: string }
interface ToastContext {
  readonly items: Toast[];
  push(t: { kind, text }): void;
  dismiss(id: number): void;
}
```

Auto-dismiss after 4.5 s. Position: fixed bottom-right.

---

### Motion (Svelte actions)

#### `use:inView` — fire callback when element enters viewport

```svelte
<div use:inView={() => (visible = true)}>…</div>
```

One-shot. Useful for triggering CSS transitions on appear without
managing an `IntersectionObserver` per call site.

#### `use:textScramble` — hacker-scramble text on appear

```svelte
<h1 use:textScramble={{ text: "Security audit", duration: 600, stagger: 40 }}>
  Security audit
</h1>
```

| opt       | type   | default | description                       |
|-----------|--------|---------|-----------------------------------|
| `text`    | string | —       | final text (action takes over node content) |
| `duration`| number | `600`   | total scramble duration (ms)      |
| `stagger` | number | `40`    | per-character offset (ms)         |

#### `use:scrambleCounter` — numeric/symbol counter scramble

```svelte
<span use:scrambleCounter={{ value: "130+" }}>0</span>
```

If `value` matches `/^(\d+)(.*)$/` → counts up to `\d+` with rolling
digits, then appends the suffix. Otherwise cycles through `∑∆Ω∏…` and
settles on `value`.

---

### Theme tokens

```ts
import {
  STATUS_TINT,            // scan/agent lifecycle → CSS var
  SEVERITY_COLORS,        // finding severity → hex
  SEVERITY_ORDER,         // ordered keys for severity
  CONFIDENCE_COLORS,      // certain/firm/tentative + high/medium/low aliases
  METHOD_COLORS,          // HTTP method → hex
  statusColor,            // (code) => hex for HTTP response codes
  statusTint,             // lookup with var(--aim-text-muted) fallback
  severityColor,          // lookup with fallback
  confidenceColor,        // lookup with fallback
  methodColor,            // lookup with var(--aim-text) fallback
} from "@landing-v/ui";
```

All lowercase keys. Looking up an unknown key returns the fallback
(`var(--aim-text-muted)` for most; `var(--aim-text)` for `methodColor`).

#### `STATUS_TINT`
| key | colour |
|-----|--------|
| `running`   | `var(--aim-success)` |
| `completed` | `var(--aim-accent)`  |
| `paused`    | `var(--aim-tertiary)` |
| `failed`    | `var(--aim-error)`   |
| `pending`   | `var(--aim-text-muted)` |

#### `SEVERITY_COLORS`
| key | hex |
|-----|-----|
| `critical` | `#E53935` |
| `high`     | `#EF5350` |
| `medium`   | `#FFA726` |
| `low`      | `#FFD54F` |
| `suspect`  | `#AB47BC` |
| `info`     | `#42A5F5` |

#### `METHOD_COLORS`
| method | hex |
|--------|-----|
| `GET`     | `#98bc37` |
| `POST`    | `#68a8e4` |
| `PUT`     | `#FFA726` |
| `DELETE`  | `#E53935` |
| `PATCH`   | `#68a8e4` |

`statusColor(code)`: `≥500 → #E53935`, `≥400 → #FFA726`, `≥300 → #68a8e4`,
else `#98bc37`.

---

### Formatters

```ts
fmtTime(iso?: string | null): string   // "May 28, 01:17" or "—"
fmtDur(ms: number): string             // "1.2s" or "3m 14s" or "—"
```

---

## Theme variables (defined in each app's `app.css`)

```css
--aim-bg          /* page background */
--aim-surface     /* elevated surface (table headers, tooltips) */
--aim-bg-elev     /* slightly raised panel */
--aim-text        /* primary text */
--aim-text-muted  /* secondary text */
--aim-text-dim    /* faintest text */
--aim-accent      /* terminal green (primary) */
--aim-secondary   /* blue (links, info) */
--aim-tertiary    /* orange (warnings, paused) */
--aim-border      /* hairline border */
--aim-line        /* divider lines */
--aim-line-soft   /* fainter divider */
--aim-success     /* green (running) */
--aim-error       /* red (failed) */
--aim-warn        /* yellow */
--aim-danger      /* alias for error */
```

Override per theme by adding `[data-theme="my-theme"] { … }` in app.css.

---

## Composing a new page

The pattern for building a new landing or console page from primitives:

### Landing page section

```svelte
<script lang="ts">
  import { Section, SectionHeading, CardGrid, Card, CornerBrackets } from "@landing-v/ui";

  const items = [
    { title: "First",  body: "…" },
    { title: "Second", body: "…" },
    { title: "Third",  body: "…" },
  ];
</script>

<Section id="my-section">
  <SectionHeading annotation="Kicker" heading="Big Title" />
  <CardGrid items={items} cols={3} stagger={120}>
    {#snippet item(it, i)}
      <Card tilt class="h-full">
        <CornerBrackets />
        <h3 class="aim-heading">{it.title}</h3>
        <p class="aim-body">{it.body}</p>
      </Card>
    {/snippet}
  </CardGrid>
</Section>
```

### Console list page

```svelte
<script lang="ts">
  import PageShell from "$lib/components/PageShell.svelte";
  import { Activity } from "lucide-svelte";
  import { useThings } from "$lib/api/hooks";
  import {
    ListPage, StickyTable, type Column,
    FilterChips, SearchBox, BracketButton, STATUS_TINT, fmtTime,
  } from "@landing-v/ui";

  let statusFilter = $state("");
  let search       = $state("");
  let offset       = $state(0);
  const q = useThings({ limit: 100, offset });

  const FILTERS = [
    { v: "", label: "all" },
    { v: "active", label: "active" },
    { v: "archived", label: "archived" },
  ];
  const COLS: Column[] = [
    { key: "status", label: "STATUS" },
    { key: "name",   label: "NAME" },
    { key: "actions", label: "ACTIONS" },
  ];

  let filtered = $derived(/* … filter logic … */);
</script>

<PageShell>
  <ListPage
    title="THINGS"
    titleIcon={Activity}
    isFetching={q.isFetching}
    onRefresh={() => q.refetch()}
    total={q.data?.total}
    bind:offset
  >
    {#snippet toolbarLeft()}
      <div class="ml-2"><FilterChips bind:value={statusFilter} options={FILTERS} /></div>
    {/snippet}
    {#snippet toolbarRight()}
      <SearchBox bind:value={search} placeholder="search..." />
    {/snippet}
    {#snippet body()}
      <StickyTable columns={COLS} items={filtered} empty="no things">
        {#snippet row(t)}
          <tr style="border-bottom: 1px solid var(--aim-border);">
            <td class="px-2 py-1" style="color: {STATUS_TINT[t.status]};">{t.status}</td>
            <td class="px-2 py-1">{t.name}</td>
            <td class="px-2 py-1">
              <BracketButton tone="error">[delete]</BracketButton>
            </td>
          </tr>
        {/snippet}
      </StickyTable>
    {/snippet}
  </ListPage>
</PageShell>
```

That's the entire shape — about 50 lines, no chrome duplication.

---

## When to add a new component vs. extend an existing one

- **Add a new component** when the new pattern doesn't visually share
  more than 30 % of styling with anything already here, OR it has
  fundamentally different props.
- **Extend (more props/snippets)** when 2+ existing consumers add the
  same inline workaround on top of an existing component.
- **Inline (don't share)** when only one consumer needs it, even if it
  looks similar to something existing. The "rule of 3" applies: wait
  for a third use case before abstracting.

## Adding a component

1. Drop the `.svelte` file into `src/lib/components/`.
2. Re-export it from `src/index.ts`.
3. Add the JSDoc block at the top with usage example.
4. Append to this README — props table + example + notes.
5. Run `node scripts/hash-inline-styles.mjs` from the repo root so the
   component's inline styles are folded into hashed classes in every
   consuming app's `_inline-styles.css`.
6. Smoke-test all 14 console routes + landing (`curl … :9011` /
   `:9010`) before committing.

## Fingerprint-defeat pipeline

This package participates in the
[`scripts/hash-inline-styles.mjs`](../../scripts/hash-inline-styles.mjs)
build step. Every `style="…"` in any package component is turned into a
salted-hash class name (`s-XXXXXXX`) at compile time, and value-level
forms like `clamp(a, b, c)` are rewritten to equivalent `min(c, max(a, b))`
to keep the rendered CSS textually unique per build. See the script's
header comment for run forms and salt overrides.
