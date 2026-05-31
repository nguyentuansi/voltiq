// Public surface of @landing-v/ui. Both apps import from here:
//   import { SectionHeading, ToastStack, inView } from "@landing-v/ui";
//
// Add new shared components by exporting them below. Internal helpers live
// in `lib/utils/` and ship through the `./utils/*` subpath export.

export { default as SectionHeading } from "./lib/components/SectionHeading.svelte";
export { default as CornerBrackets } from "./lib/components/CornerBrackets.svelte";
export { default as ToastStack }     from "./lib/components/ToastStack.svelte";
export { default as ListPage }       from "./lib/components/ListPage.svelte";
export { default as StickyTable }    from "./lib/components/StickyTable.svelte";
export type { Column }               from "./lib/components/StickyTable.svelte";
export { default as Card }           from "./lib/components/Card.svelte";
export { default as CardGrid }       from "./lib/components/CardGrid.svelte";
export { default as Section }        from "./lib/components/Section.svelte";
export { default as BracketButton }  from "./lib/components/BracketButton.svelte";
export { default as IconButton }     from "./lib/components/IconButton.svelte";
export { default as SearchBox }      from "./lib/components/SearchBox.svelte";
export { default as FilterChips }    from "./lib/components/FilterChips.svelte";
export { default as Select }         from "./lib/components/Select.svelte";
export { default as RegistrationMark } from "./lib/components/RegistrationMark.svelte";
export { default as AccentLink }     from "./lib/components/AccentLink.svelte";
export { default as Stat }           from "./lib/components/Stat.svelte";
export { default as AccordionItem }  from "./lib/components/AccordionItem.svelte";
export { default as ComparisonTable } from "./lib/components/ComparisonTable.svelte";
export type { ComparisonColumn }     from "./lib/components/ComparisonTable.svelte";

// Toast context (paired with ToastStack).
export {
  setToastContext,
  getToastContext,
  type Toast,
  type ToastKind,
  type ToastContext,
} from "./lib/contexts/toast.svelte";

// View-triggered animation actions.
export {
  inView,
  textScramble,
  scrambleCounter,
  type TextScrambleOpts,
  type ScrambleCounterOpts,
} from "./lib/utils/animations";

// Shared colour palettes and lookups (status/severity/method/etc).
export {
  STATUS_TINT,
  SEVERITY_COLORS,
  SEVERITY_ORDER,
  CONFIDENCE_COLORS,
  METHOD_COLORS,
  statusColor,
  statusTint,
  severityColor,
  confidenceColor,
  methodColor,
} from "./lib/utils/colors";

// Time / duration formatters.
export { fmtTime, fmtDur } from "./lib/utils/format";
