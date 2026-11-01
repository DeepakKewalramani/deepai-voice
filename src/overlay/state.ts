/** Look of the notch pill. */
export type OverlayMode = "idle" | "listening" | "thinking" | "error";

/** Where the pill sits on screen (set from the tray or settings). */
export type OverlayPosition = "top" | "bottom" | "left" | "right";

export interface OverlayState {
  mode: OverlayMode;
  /** Accessible name of the state (the pill itself shows no text). */
  label: string;
}

export const IDLE: OverlayState = { mode: "idle", label: "" };

/** How long an error stays on the pill before it calms back to idle. */
export const ERROR_HOLD_MS = 4000;

/**
 * Maps the backend status text (see `Machine::status_text` in
 * src-tauri/src/dictation/state.rs) to a pill state. Empty is idle, "REC" is
 * listening, "..." is transcribing or pasting, anything else is an error.
 */
export function overlayStateFromText(text: unknown): OverlayState {
  const t = typeof text === "string" ? text.trim() : "";
  if (t === "") return IDLE;
  if (t === "REC") return { mode: "listening", label: "Listening" };
  if (t === "...") return { mode: "thinking", label: "Thinking" };
  return { mode: "error", label: t };
}

const POSITIONS: readonly OverlayPosition[] = [
  "top",
  "bottom",
  "left",
  "right",
];

/** Parses a position name, falling back to the notch (top). */
export function positionFromText(text: unknown): OverlayPosition {
  const t = typeof text === "string" ? text.trim().toLowerCase() : "";
  return (POSITIONS as readonly string[]).includes(t)
    ? (t as OverlayPosition)
    : "top";
}

/** True when the page was opened as the overlay (`?view=overlay`). */
export function isOverlaySearch(search: string): boolean {
  return new URLSearchParams(search).get("view") === "overlay";
}

/** Capsule size in px for each look (idle and error are the small plain
 * capsule; listening and thinking grow to hold the wave). Keep in sync with
 * `DEFAULT_CAPSULE` and the window size in src-tauri/src/overlay.rs. */
export const CAPSULE_SIZES: Record<
  OverlayMode,
  { width: number; height: number }
> = {
  idle: { width: 60, height: 30 },
  error: { width: 60, height: 30 },
  thinking: { width: 92, height: 34 },
  listening: { width: 92, height: 34 },
};

export function capsuleSize(mode: OverlayMode): {
  width: number;
  height: number;
} {
  return CAPSULE_SIZES[mode];
}

/** Wave drawing box inside the capsule, in px. */
export const WAVE_W = 66;
export const WAVE_H = 22;
/** Short flat line shown in the small error capsule. */
export const ERROR_LINE_W = 30;

/**
 * Wave amplitude in px at time `t` (ms). Listening breathes slowly, or
 * follows the mic `level` (0..1) when one is known; thinking is a low
 * shimmer; idle and error are flat.
 */
export function waveAmplitude(
  mode: OverlayMode,
  t: number,
  level: number | null = null,
): number {
  switch (mode) {
    case "listening":
      if (level !== null) return 7 * (0.25 + 0.75 * clamp01(level));
      return 7 * (0.6 + 0.4 * Math.sin(t / 420));
    case "thinking":
      return 1.2 + 0.4 * Math.sin(t / 300);
    default:
      return 0;
  }
}

function clamp01(v: number): number {
  return Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : 0;
}

/**
 * SVG path for one sine line across `width`. The envelope
 * sin(pi * x / width) keeps both ends calm at the midline. `speed` scales
 * how fast the wave travels.
 */
export function sinePath(
  t: number,
  amplitude: number,
  width = WAVE_W,
  height = WAVE_H,
  speed = 1,
): string {
  const mid = height / 2;
  const parts: string[] = [];
  for (let x = 0; x <= width; x += 2) {
    const env = Math.sin((x / width) * Math.PI);
    const y = mid + Math.sin(x / 5 + (t * speed) / 160) * amplitude * env;
    parts.push(`${x === 0 ? "M" : "L"}${x} ${y.toFixed(2)}`);
  }
  return parts.join(" ");
}

/** A mic level event payload as 0..1, or null when it is not a number. */
export function levelFromPayload(payload: unknown): number | null {
  return typeof payload === "number" && Number.isFinite(payload)
    ? clamp01(payload)
    : null;
}

/** The `overlay-drag` event payload: true only for `{dragging: true}`. */
export function draggingFromPayload(payload: unknown): boolean {
  return (
    typeof payload === "object" &&
    payload !== null &&
    (payload as { dragging?: unknown }).dragging === true
  );
}

/**
 * Dev preview only: `?state=listening|thinking|error|idle` forces a state
 * and `?pos=left` a position, so the pill can be screenshotted without
 * Tauri. `partial` is kept for src/main.tsx; the capsule shows no text.
 */
export function previewFromSearch(search: string): {
  state: OverlayState | null;
  position: OverlayPosition | null;
  partial: string;
} {
  const p = new URLSearchParams(search);
  const s = p.get("state");
  const byName: Record<string, string> = {
    idle: "",
    listening: "REC",
    thinking: "...",
    error: p.get("msg") ?? "Mic denied in Settings",
  };
  return {
    state: s !== null && s in byName ? overlayStateFromText(byName[s]) : null,
    position: p.has("pos") ? positionFromText(p.get("pos")) : null,
    partial: "",
  };
}
