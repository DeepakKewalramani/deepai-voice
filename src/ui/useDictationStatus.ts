import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { DictationStatus, READY, statusFromText } from "./status";

/** Event the Rust dictation orchestrator emits with the status text. */
export const DICTATION_STATUS_EVENT = "dictation-status";

function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Live dictation status from the backend. Outside the Tauri webview (plain
 * browser, tests) it stays Ready.
 */
export function useDictationStatus(): DictationStatus {
  const [status, setStatus] = useState<DictationStatus>(READY);

  useEffect(() => {
    if (!inTauri()) return;
    let active = true;
    let unlisten: (() => void) | undefined;
    listen<string>(DICTATION_STATUS_EVENT, (e) => {
      setStatus(statusFromText(typeof e.payload === "string" ? e.payload : ""));
    }).then(
      (fn) => {
        if (active) unlisten = fn;
        else fn();
      },
      () => {
        // Listening failed: keep the static Ready state.
      },
    );
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  return status;
}
