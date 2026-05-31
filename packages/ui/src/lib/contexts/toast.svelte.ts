/**
 * Toast context — global notification queue.
 *
 *   const toast = getToastContext();
 *   toast.push({ kind: "success", text: "Scan started" });
 */

import { getContext, setContext } from "svelte";

const KEY = Symbol("toast");

export type ToastKind = "success" | "error" | "info";
export interface Toast {
  id: number;
  kind: ToastKind;
  text: string;
}

export interface ToastContext {
  readonly items: Toast[];
  push: (t: { kind: ToastKind; text: string }) => void;
  dismiss: (id: number) => void;
}

export function setToastContext(): ToastContext {
  let items = $state<Toast[]>([]);
  let nextId = 0;
  const ctx: ToastContext = {
    get items() { return items; },
    push(t) {
      const id = ++nextId;
      items = [...items, { id, ...t }];
      setTimeout(() => { items = items.filter((x) => x.id !== id); }, 4500);
    },
    dismiss(id) { items = items.filter((x) => x.id !== id); },
  };
  setContext(KEY, ctx);
  return ctx;
}

export function getToastContext(): ToastContext {
  const ctx = getContext<ToastContext>(KEY);
  if (!ctx) throw new Error("Toast context not set.");
  return ctx;
}
