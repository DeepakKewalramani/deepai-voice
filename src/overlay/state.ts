/** Look of the notch pill. */
export type OverlayMode = "idle" | "listening" | "thinking" | "error";

/** Where the pill sits on screen (set from the tray or settings). */
export type OverlayPosition = "top" | "bottom" | "left" | "right";

export interface OverlayState {
  mode: OverlayMode;
  /** Short word or message shown in the pill. Empty when idle. */
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

/**
 * Dev preview only: `?state=listening|thinking|error|idle` forces a state and
 * `?pos=left` a position, so the pill can be screenshotted without Tauri.
 */
export function previewFromSearch(search: string): {
  state: OverlayState | null;
  position: OverlayPosition | null;
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
  };
}
