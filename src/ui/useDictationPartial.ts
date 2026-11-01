import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

/** Event with the live (not yet typed) dictation text; empty clears it. */
export const DICTATION_PARTIAL_EVENT = "dictation-partial";

function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Live dictation text from the backend, held only in memory. Outside the
 * Tauri webview (plain browser, tests) it stays empty.
 */
export function useDictationPartial(): string {
  const [partial, setPartial] = useState("");

  useEffect(() => {
    if (!inTauri()) return;
    let active = true;
    let unlisten: (() => void) | undefined;
    listen<string>(DICTATION_PARTIAL_EVENT, (e) => {
      setPartial(typeof e.payload === "string" ? e.payload : "");
    }).then(
      (fn) => {
        if (active) unlisten = fn;
        else fn();
      },
      () => {
        // Listening failed: no live text.
      },
    );
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  return partial;
}
