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
