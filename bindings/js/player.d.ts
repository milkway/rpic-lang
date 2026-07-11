// Type definitions for the standalone rpic player (player.js — zero imports,
// no wasm). `animate`/`interactive` are also re-exported by the package root.

export interface Anim {
  id: string;
  /** "fade" | "pop" | "draw" | "slide" | "move" | "highlight" | "morph" | "type" | "scramble" | "wiggle" */
  effect: string;
  /** absolute start time in seconds */
  start: number;
  duration: number;
  /** replay count; -1 loops forever. Present only when set. */
  repeat?: number;
  /** reverse on each repeat. Present only when set. */
  yoyo?: boolean;
  /** GSAP easing name overriding the effect's default. Present only when set. */
  ease?: string;
  /** id of the object whose path a `move` follows. Present only for `move`. */
  path?: string;
  /** target colour for `highlight` (`#rrggbb` or a name). Present only when set. */
  color?: string;
  /** play the effect as an exit (reverse) instead of an entrance. */
  out?: boolean;
  /** entry direction for `slide` ("up" | "down" | "left" | "right"). */
  from?: string;
  /** id of the shape a `morph` tweens into. Present only for `morph`. */
  morph?: string;
  /** `type` reveal granularity: "word" splits by word, absent means by char. */
  unit?: string;
  /** custom scramble charset for `scramble` (`by "…"`). Present only when set. */
  chars?: string;
  /** oscillation count for `wiggle` (`wiggles <n>`). Present only when set. */
  wiggles?: number;
  /** `draw` reveal start as a stroke fraction (`from 40%` → 0.4). Present only when set. */
  drawFrom?: number;
  /** `draw` reveal end as a stroke fraction (`to 60%` → 0.6). Present only when set. */
  drawTo?: number;
}

export interface Interaction {
  id: string;
  /** currently always "drag" */
  kind: string;
  /** throw with momentum (needs InertiaPlugin). Present only when set. */
  inertia?: boolean;
  /** id of the shape whose box constrains dragging. Present only when set. */
  bounds?: string;
  /** axis lock: "x" | "y". Present only when set (absent = free drag). */
  axis?: string;
}

/**
 * Build and play a GSAP timeline from an animation manifest on the SVG inside
 * `root`. Browser-only.
 */
export function animate(root: Element, animations: Anim[], gsap: unknown): unknown;

/**
 * Make objects draggable from the `interactions` manifest (the `draggable`
 * directive). Pass GSAP's `Draggable` class (register it — and `InertiaPlugin`
 * if any interaction uses `inertia`). Returns the created Draggable instances.
 * Browser-only.
 */
export function interactive(root: Element, interactions: Interaction[], Draggable: unknown): unknown[];
