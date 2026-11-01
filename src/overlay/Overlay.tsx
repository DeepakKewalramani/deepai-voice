import { CSSProperties, useEffect, useRef, useState } from "react";
import "./overlay.css";
import {
  ERROR_HOLD_MS,
  ERROR_LINE_W,
  IDLE,
  OverlayMode,
  OverlayPosition,
  OverlayState,
  WAVE_H,
  WAVE_W,
  capsuleSize,
  draggingFromPayload,
  isVertical,
  levelFromPayload,
  overlayStateFromText,
  positionFromText,
  sinePath,
  waveAmplitude,
} from "./state";

/** Event the Rust dictation orchestrator emits with the status text. */
export const DICTATION_STATUS_EVENT = "dictation-status";
/** Optional mic level event (a number 0..1). Without it the wave loops. */
export const DICTATION_LEVEL_EVENT = "dictation-level";
/** Event Rust sends when the pill is moved (payload "top" etc). */
export const OVERLAY_POSITION_EVENT = "overlay-position";
/** Event Rust sends while the pill is dragged: `{ dragging: bool }`. */
export const OVERLAY_DRAG_EVENT = "overlay-drag";
/** Event the pill sends with its capsule size, for the drag hit test. */
export const OVERLAY_CAPSULE_EVENT = "overlay-capsule";
/** Dev-only window event: `{ status?, position? }`. */
export const OVERLAY_PREVIEW_EVENT = "overlay-preview";

/** Static gentle wave for reduced motion and the first paint. */
const STILL_AMPLITUDE = 3.5;
/** Thinking travels slower than listening. */
const THINKING_SPEED = 0.35;

export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function prefersReducedMotion(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

/** The first, static path for a mode (also the reduced motion drawing). */
function stillPath(mode: OverlayMode): string {
  if (mode === "error") return sinePath(0, 0, ERROR_LINE_W);
  if (mode === "thinking") return sinePath(0, waveAmplitude("thinking", 0));
  return sinePath(0, STILL_AMPLITUDE);
}

interface Props {
  initialState?: OverlayState;
  initialPosition?: OverlayPosition;
  /** Ignored: the capsule shows no text. Kept for src/main.tsx. */
  initialPartial?: string;
  initialDragging?: boolean;
}

/** DeepAI sine capsule: a plain graphite capsule that shows one moving
 * sine line while listening, shown in the transparent `overlay` window. On
 * the left and right anchors CSS turns the whole capsule (wave included)
 * 90 degrees; the wave is always drawn along the capsule's own x axis. */
export default function Overlay({
  initialState = IDLE,
  initialPosition = "top",
  initialDragging = false,
}: Props) {
  const [state, setState] = useState<OverlayState>(initialState);
  const [position, setPosition] = useState<OverlayPosition>(initialPosition);
  const [dragging, setDragging] = useState<boolean>(initialDragging);
  const pathRef = useRef<SVGPathElement>(null);
  const levelRef = useRef<number | null>(null);

  // Live status, position, drag and mic level from the backend.
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
          listen<unknown>(OVERLAY_DRAG_EVENT, (e) =>
            setDragging(draggingFromPayload(e.payload)),
          ),
          listen<unknown>(DICTATION_LEVEL_EVENT, (e) => {
            levelRef.current = levelFromPayload(e.payload);
          }),
        ]);
        if (active) unlisteners.push(...subs);
        else subs.forEach((fn) => fn());
        const pos = await invoke<string>("get_overlay_position");
        if (active) setPosition(positionFromText(pos));
      } catch {
        // No backend: stay idle at the top.
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
  const size = capsuleSize(mode);

  // Tell Rust the capsule size so a press is only grabbed on the capsule.
  useEffect(() => {
    if (!inTauri()) return;
    void import("@tauri-apps/api/event")
      .then(({ emit }) => emit(OVERLAY_CAPSULE_EVENT, size))
      .catch(() => {});
  }, [size]);

  // Animate the wave only while listening or thinking, only while the page
  // is visible, and never with reduced motion (the still path stays).
  useEffect(() => {
    if (mode !== "listening" && mode !== "thinking") return;
    if (prefersReducedMotion()) return;
    const speed = mode === "thinking" ? THINKING_SPEED : 1;
    let raf = 0;
    const tick = (t: number) => {
      raf = 0;
      if (document.hidden) return;
      pathRef.current?.setAttribute(
        "d",
        sinePath(
          t,
          waveAmplitude(mode, t, levelRef.current),
          WAVE_W,
          WAVE_H,
          speed,
        ),
      );
      raf = requestAnimationFrame(tick);
    };
    const start = () => {
      if (raf === 0 && !document.hidden) raf = requestAnimationFrame(tick);
    };
    document.addEventListener("visibilitychange", start);
    start();
    return () => {
      document.removeEventListener("visibilitychange", start);
      if (raf !== 0) cancelAnimationFrame(raf);
    };
  }, [mode]);

  const style = {
    "--w": `${size.width}px`,
    "--h": `${size.height}px`,
  } as CSSProperties;
  const lineW = mode === "error" ? ERROR_LINE_W : WAVE_W;
  return (
    <div
      className="ov"
      data-pos={position}
      data-orient={isVertical(position) ? "vertical" : "horizontal"}
      data-mode={mode}
      data-dragging={dragging ? "true" : undefined}
    >
      <div
        className="ov-pill"
        style={style}
        role="status"
        aria-live="polite"
        aria-label={mode === "idle" ? "DeepAI Voice ready" : label}
      >
        {mode !== "idle" && (
          <svg
            className="ov-wave"
            width={lineW}
            height={WAVE_H}
            viewBox={`0 0 ${lineW} ${WAVE_H}`}
            aria-hidden="true"
          >
            <defs>
              <linearGradient
                id="ov-sine-grad"
                gradientUnits="userSpaceOnUse"
                x1="0"
                y1="0"
                x2={WAVE_W}
                y2="0"
              >
                <stop offset="0" stopColor="#ff8a4c" />
                <stop offset="1" stopColor="#b86bff" />
              </linearGradient>
            </defs>
            <path ref={pathRef} className="ov-line" d={stillPath(mode)} />
          </svg>
        )}
      </div>
    </div>
  );
}
