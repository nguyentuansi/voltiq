/**
 * Svelte actions for view-triggered animations.
 *
 *   - `inView`         — fires a callback once the element enters the viewport.
 *   - `textScramble`   — hacker-style scramble into the final text.
 *   - `scrambleCounter`— numeric/symbol scramble counter.
 */

const SCRAMBLE_CHARS = "!@#$%^&*()_+-=[]{}|;:,.<>?/~`01";
const DIGIT_CHARS    = "0123456789";

// ── inView action ─────────────────────────────────────────────────────
//
//   <h2 use:inView={() => (visible = true)}>...</h2>
//
// One-shot — observer disconnects after the first intersection.
export function inView(node: HTMLElement, callback: () => void) {
  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          callback();
          observer.disconnect();
          return;
        }
      }
    },
    { threshold: 0.1 },
  );
  observer.observe(node);
  return { destroy() { observer.disconnect(); } };
}

// ── textScramble action ──────────────────────────────────────────────
//
//   <h1 use:textScramble={{ text: "Serious security audit", duration: 250, stagger: 15 }}>
//     <!-- initial text or empty -->
//   </h1>
//
// The action takes ownership of the element's text content. It fills it
// with random glyphs and gradually reveals the real letters, left to right.
export interface TextScrambleOpts {
  text:     string;
  duration?: number; // total ms
  stagger?:  number; // ms per character offset
}

export function textScramble(node: HTMLElement, opts: TextScrambleOpts) {
  let started = false;
  let rafId: number | null = null;
  const final = opts.text;
  const duration = opts.duration ?? 600;
  const stagger  = opts.stagger  ?? 40;
  node.textContent = final;

  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting || started) continue;
        started = true;
        observer.disconnect();
        const start = performance.now();
        const chars = final.split("");
        const total = duration + chars.length * stagger;

        const tick = (now: number) => {
          const elapsed = now - start;
          if (elapsed >= total) { node.textContent = final; return; }
          const out = chars.map((ch, i) => {
            if (ch === " ") return " ";
            const p = Math.max(0, elapsed - i * stagger) / duration;
            return p >= 1 ? ch : SCRAMBLE_CHARS[Math.floor(Math.random() * SCRAMBLE_CHARS.length)];
          });
          node.textContent = out.join("");
          rafId = requestAnimationFrame(tick);
        };
        rafId = requestAnimationFrame(tick);
        return;
      }
    },
    { threshold: 0.1 },
  );
  observer.observe(node);

  return {
    destroy() {
      observer.disconnect();
      if (rafId !== null) cancelAnimationFrame(rafId);
    },
  };
}

// ── scrambleCounter action ───────────────────────────────────────────
//
//   <span use:scrambleCounter={{ value: "130+" }}>0</span>
//
// Numeric value: counts up with random-digit flicker (1.2s). Non-numeric
// value (e.g. "∞"): cycles through math symbols then settles.
export interface ScrambleCounterOpts {
  value: string;
}

export function scrambleCounter(node: HTMLElement, opts: ScrambleCounterOpts) {
  let started = false;
  let rafId: number | null = null;
  const { value } = opts;
  node.textContent = "0";

  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting || started) continue;
        started = true;
        observer.disconnect();
        runCounter(node, value, (id) => { rafId = id; });
        return;
      }
    },
    { threshold: 0.1 },
  );
  observer.observe(node);

  return {
    destroy() {
      observer.disconnect();
      if (rafId !== null) cancelAnimationFrame(rafId);
    },
  };
}

function runCounter(node: HTMLElement, value: string, capture: (id: number) => void) {
  const match = value.match(/^(\d+)(.*)$/);
  if (!match) {
    const scrambleDuration = 800;
    const start = performance.now();
    const symbols = "∑∆Ω∏∫√∞≈≠±";
    const tick = (now: number) => {
      if (now - start >= scrambleDuration) { node.textContent = value; return; }
      node.textContent = symbols[Math.floor(Math.random() * symbols.length)];
      capture(requestAnimationFrame(tick));
    };
    capture(requestAnimationFrame(tick));
    return;
  }
  const target  = parseInt(match[1], 10);
  const suffix  = match[2];
  const total   = 1200;
  const scrambleEnd = 400;
  const start   = performance.now();
  const tick = (now: number) => {
    const elapsed = now - start;
    if (elapsed < scrambleEnd) {
      let s = "";
      for (let i = 0; i < String(target).length; i++) {
        s += DIGIT_CHARS[Math.floor(Math.random() * 10)];
      }
      node.textContent = `${s}${suffix}`;
      capture(requestAnimationFrame(tick));
      return;
    }
    const p = Math.min((elapsed - scrambleEnd) / (total - scrambleEnd), 1);
    const eased = 1 - Math.pow(1 - p, 3);
    const cur = Math.round(eased * target);
    const curStr = String(cur).padStart(String(target).length, "0");
    const tgtStr = String(target);
    let result = "";
    for (let i = 0; i < tgtStr.length; i++) {
      result += p > 0.85 || curStr[i] === tgtStr[i] ? curStr[i] : DIGIT_CHARS[Math.floor(Math.random() * 10)];
    }
    node.textContent = `${parseInt(result, 10)}${suffix}`;
    if (p < 1) capture(requestAnimationFrame(tick));
    else node.textContent = `${target}${suffix}`;
  };
  capture(requestAnimationFrame(tick));
}
