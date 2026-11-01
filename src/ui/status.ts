/** Dictation state as shown in the status card. */
export type DictationKind = "ready" | "recording" | "transcribing" | "error";

export interface DictationStatus {
  kind: DictationKind;
  /** Backend message for the error state, empty otherwise. */
  message: string;
}

export const READY: DictationStatus = { kind: "ready", message: "" };

/**
 * Maps the backend status text (the same text the tray title shows, see
 * `Machine::status_text` in src-tauri/src/dictation/state.rs) to a card state.
 * Empty is idle, "REC" is recording, "..." is transcribing or pasting, and any
 * other text is an error message.
 */
export function statusFromText(text: string): DictationStatus {
  const t = text.trim();
  if (t === "") return READY;
  if (t === "REC") return { kind: "recording", message: "" };
  if (t === "...") return { kind: "transcribing", message: "" };
  return { kind: "error", message: t };
}

/** Most characters of live text the status card shows. */
export const LIVE_LINE_MAX = 120;

/**
 * The quiet "Live" line for the status card: the not yet typed words while
 * recording, as one line ellipsized at the start. Empty in any other state.
 */
export function liveLine(status: DictationStatus, partial: unknown): string {
  if (status.kind !== "recording" || typeof partial !== "string") return "";
  const chars = Array.from(partial.replace(/\s+/g, " ").trim());
  if (chars.length <= LIVE_LINE_MAX) return chars.join("");
  return (
    "…" +
    chars
      .slice(chars.length - (LIVE_LINE_MAX - 1))
      .join("")
      .trimStart()
  );
}
