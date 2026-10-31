import { useEffect, useState } from "react";
import "./overlay.css";
import {
  ERROR_HOLD_MS,
  IDLE,
  OverlayPosition,
  OverlayState,
  overlayStateFromText,
  positionFromText,
} from "./state";

/** Event the Rust dictation orchestrator emits with the status text. */
export const DICTATION_STATUS_EVENT = "dictation-status";
/** Event Rust sends when the pill is moved (payload "top" etc). */
export const OVERLAY_POSITION_EVENT = "overlay-position";
/** Dev-only window event: `{ status?: string, position?: string }`. */
export const OVERLAY_PREVIEW_EVENT = "overlay-preview";

const WAVE_BARS = 7;

export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

interface Props {
  initialState?: OverlayState;
  initialPosition?: OverlayPosition;
}

/** Notch-style status pill rendered in the transparent `overlay` window. */
export default function Overlay({
  initialState = IDLE,
  initialPosition = "top",
}: Props) {
  const [state, setState] = useState<OverlayState>(initialState);
  const [position, setPosition] = useState<OverlayPosition>(initialPosition);

  // Live status and position from the backend.
  useEffect(() => {
    if (!inTauri()) return;
    let active = true;
    const unlisteners: Array<() => void> = [];
    void (async () => {
      try {
        const [{ listen }, { invoke }] = await Promise.all([
          import("@tauri-apps/api/event"),
          import("@tauri-apps/api/core"),
        ]);
        const subs = await Promise.all([
          listen<string>(DICTATION_STATUS_EVENT, (e) =>
            setState(overlayStateFromText(e.payload)),
          ),
          listen<string>(OVERLAY_POSITION_EVENT, (e) =>
            setPosition(positionFromText(e.payload)),
          ),
        ]);
        if (active) unlisteners.push(...subs);
        else subs.forEach((fn) => fn());
        const pos = await invoke<string>("get_overlay_position");
        if (active) setPosition(positionFromText(pos));
      } catch {
        // No backend: stay idle at the notch.
      }
    })();
    return () => {
      active = false;
      unlisteners.forEach((fn) => fn());
    };
  }, []);

  // Dev preview hook, so the pill can be driven from a plain browser.
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    const onPreview = (e: Event) => {
      const d =
        (e as CustomEvent<{ status?: string; position?: string }>).detail ?? {};
      if (d.status !== undefined) setState(overlayStateFromText(d.status));
      if (d.position !== undefined) setPosition(positionFromText(d.position));
    };
    window.addEventListener(OVERLAY_PREVIEW_EVENT, onPreview);
    return () => window.removeEventListener(OVERLAY_PREVIEW_EVENT, onPreview);
  }, []);

  // Errors show briefly, then the pill calms back to idle.
  useEffect(() => {
    if (state.mode !== "error") return;
    const t = window.setTimeout(() => setState(IDLE), ERROR_HOLD_MS);
    return () => window.clearTimeout(t);
  }, [state]);

  const { mode, label } = state;
  return (
    <div className="ov" data-pos={position} data-mode={mode}>
      <div
        className="ov-pill"
        role="status"
        aria-live="polite"
        aria-label={mode === "idle" ? "DeepAI Voice ready" : label}
      >
        <div className="ov-bg" aria-hidden="true" />
        <div className="ov-content" key={mode}>
          {mode === "idle" && (
            <span className="ov-idle-dot" aria-hidden="true" />
          )}
          {mode === "listening" && (
            <>
              <span className="ov-rec" aria-hidden="true" />
              <span className="ov-label">{label}</span>
              <span className="ov-wave" aria-hidden="true">
                {Array.from({ length: WAVE_BARS }, (_, i) => (
                  <i
                    key={i}
                    style={{ animationDelay: `${(i * 137) % 600}ms` }}
                  />
                ))}
              </span>
            </>
          )}
          {mode === "thinking" && (
            <>
              <span className="ov-dots" aria-hidden="true">
                <i />
                <i />
                <i />
              </span>
              <span className="ov-label ov-label-soft">{label}</span>
            </>
          )}
          {mode === "error" && (
            <>
              <span className="ov-err-dot" aria-hidden="true" />
              <span className="ov-label ov-label-err" title={label}>
                {label}
              </span>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
