import { describe, expect, it } from "vitest";
import {
  BUTTON_GAP,
  BUTTON_SIZE,
  CAPSULE_SIZES,
  HOVER_SIZE,
  IDLE,
  WAVE_AMP_MAX,
  WAVE_H,
  WAVE_W,
  WINDOW_H,
  WINDOW_W,
  capsuleSize,
  draggingFromPayload,
  hoverFromPayload,
  hoverFromSearch,
  isOverlaySearch,
  isVertical,
  levelFromPayload,
  micLabel,
  overlayStateFromText,
  positionFromText,
  previewFromSearch,
  sinePath,
  waveAmplitude,
  windowSizeFor,
} from "../state";

describe("overlayStateFromText", () => {
  it("is idle for empty or blank text", () => {
    expect(overlayStateFromText("")).toEqual(IDLE);
    expect(overlayStateFromText("   ")).toEqual(IDLE);
  });
  it("is idle for a non-string payload", () => {
    expect(overlayStateFromText(undefined)).toEqual(IDLE);
    expect(overlayStateFromText(42)).toEqual(IDLE);
  });
  it("maps REC to listening", () => {
    expect(overlayStateFromText("REC")).toEqual({
      mode: "listening",
      label: "Listening",
    });
  });
  it("maps ... to thinking", () => {
    expect(overlayStateFromText("...")).toEqual({
      mode: "thinking",
      label: "Thinking",
    });
  });
  it("shows any other text as an error message", () => {
    expect(overlayStateFromText("Mic denied in Settings")).toEqual({
      mode: "error",
      label: "Mic denied in Settings",
    });
    expect(overlayStateFromText(" Model missing ")).toEqual({
      mode: "error",
      label: "Model missing",
    });
  });
});

describe("positionFromText", () => {
  it("accepts the four presets", () => {
    for (const p of ["top", "bottom", "left", "right"])
      expect(positionFromText(p)).toBe(p);
    expect(positionFromText(" Left ")).toBe("left");
  });
  it("falls back to top", () => {
    expect(positionFromText("middle")).toBe("top");
    expect(positionFromText(null)).toBe("top");
  });
});

describe("search params", () => {
  it("detects the overlay view", () => {
    expect(isOverlaySearch("?view=overlay")).toBe(true);
    expect(isOverlaySearch("?view=main")).toBe(false);
    expect(isOverlaySearch("")).toBe(false);
  });
  it("reads a dev preview state and position", () => {
    expect(
      previewFromSearch("?view=overlay&state=listening&pos=right"),
    ).toEqual({
      state: { mode: "listening", label: "Listening" },
      position: "right",
      partial: "",
    });
    expect(previewFromSearch("?state=error&msg=Model%20missing").state).toEqual(
      {
        mode: "error",
        label: "Model missing",
      },
    );
    expect(previewFromSearch("?state=bogus")).toEqual({
      state: null,
      position: null,
      partial: "",
    });
    // The capsule shows no text, so preview text is ignored.
    expect(
      previewFromSearch("?state=listening&partial=hello%20%20world").partial,
    ).toBe("");
  });
});

/** Y values of an "M0 11 L2 11.5 ..." path. */
const ys = (d: string) =>
  d.split(" L").map((seg) => Number(seg.replace(/^M/, "").split(" ")[1]));

// 2.3c: idle is a tiny thin capsule, listening is compact, and the hover
// controls are the largest capsule. Size numbers below changed with it.
const ALL_SIZES = [...Object.values(CAPSULE_SIZES), HOVER_SIZE];

describe("capsule sizes", () => {
  it("is a tiny thin capsule when idle and a small one in error", () => {
    expect(capsuleSize("idle")).toEqual({ width: 64, height: 16 });
    expect(capsuleSize("error")).toEqual({ width: 72, height: 20 });
    // Error stays small: no bigger than the compact listening capsule.
    expect(capsuleSize("error").width).toBeLessThan(
      capsuleSize("listening").width,
    );
  });
  it("grows to hold the wave while listening or thinking", () => {
    expect(capsuleSize("listening")).toEqual({ width: 92, height: 26 });
    expect(capsuleSize("thinking")).toEqual(capsuleSize("listening"));
  });
  it("opens into the control capsule with two round buttons", () => {
    expect(HOVER_SIZE).toEqual({ width: 84, height: 36 });
    expect(BUTTON_SIZE).toBe(28);
    // Two buttons, the gap and at least 8 px of rim on each end.
    expect(2 * BUTTON_SIZE + BUTTON_GAP + 16).toBeLessThanOrEqual(
      HOVER_SIZE.width,
    );
    expect(BUTTON_SIZE + 8).toBeLessThanOrEqual(HOVER_SIZE.height);
  });
  it("fits every size in the 112 x 56 overlay window with the drag lift", () => {
    expect([WINDOW_W, WINDOW_H]).toEqual([112, 56]);
    for (const { width, height } of ALL_SIZES) {
      expect(width * 1.04 + 16).toBeLessThanOrEqual(WINDOW_W);
      expect(height * 1.04 + 16).toBeLessThanOrEqual(WINDOW_H);
    }
  });
  it("fits every size turned upright in the swapped left and right window", () => {
    for (const pos of ["left", "right"] as const) {
      const win = windowSizeFor(pos);
      expect(win).toEqual({ width: 56, height: 112 });
      for (const { width, height } of ALL_SIZES) {
        // Rotated 90 degrees: the capsule height runs across the window.
        expect(height * 1.04 + 16).toBeLessThanOrEqual(win.width);
        expect(width * 1.04 + 16).toBeLessThanOrEqual(win.height);
      }
    }
  });
  it("keeps the wave box inside the listening capsule with room to spare", () => {
    const { width, height } = capsuleSize("listening");
    expect(WAVE_W).toBeLessThan(width - 16);
    expect(WAVE_H).toBeLessThan(height - 8);
    // Crest plus half the stroke stays in the wave box.
    expect(WAVE_AMP_MAX + 1).toBeLessThanOrEqual(WAVE_H / 2);
  });
});

describe("orientation", () => {
  it("stands upright only on the left and right anchors", () => {
    expect(isVertical("left")).toBe(true);
    expect(isVertical("right")).toBe(true);
    expect(isVertical("top")).toBe(false);
    expect(isVertical("bottom")).toBe(false);
    expect(windowSizeFor("top")).toEqual({ width: 112, height: 56 });
    expect(windowSizeFor("bottom")).toEqual(windowSizeFor("top"));
  });
});

describe("sine wave", () => {
  it("spans the wave box and keeps both ends on the midline", () => {
    const d = sinePath(1234, WAVE_AMP_MAX);
    expect(d.startsWith("M0 ")).toBe(true);
    expect(d).toContain(`L${WAVE_W} `);
    const y = ys(d);
    expect(y[0]).toBeCloseTo(WAVE_H / 2, 5);
    expect(y[y.length - 1]).toBeCloseTo(WAVE_H / 2, 5);
    for (const v of y) {
      expect(v).toBeGreaterThanOrEqual(WAVE_H / 2 - WAVE_AMP_MAX);
      expect(v).toBeLessThanOrEqual(WAVE_H / 2 + WAVE_AMP_MAX);
    }
  });
  it("moves over time", () => {
    expect(sinePath(0, 7)).not.toBe(sinePath(200, 7));
  });
  it("is a flat line with no amplitude", () => {
    expect(new Set(ys(sinePath(500, 0))).size).toBe(1);
  });
  it("breathes while listening and follows a mic level when known", () => {
    const amps = [0, 300, 600, 900, 1300, 1800].map((t) =>
      waveAmplitude("listening", t),
    );
    expect(Math.min(...amps)).toBeGreaterThanOrEqual(WAVE_AMP_MAX * 0.2);
    expect(Math.max(...amps)).toBeLessThanOrEqual(WAVE_AMP_MAX);
    expect(Math.max(...amps) - Math.min(...amps)).toBeGreaterThan(1);
    expect(waveAmplitude("listening", 0, 1)).toBe(WAVE_AMP_MAX);
    expect(waveAmplitude("listening", 0, 0)).toBeCloseTo(WAVE_AMP_MAX * 0.25);
    expect(waveAmplitude("listening", 0, 5)).toBe(WAVE_AMP_MAX);
  });
  it("shimmers low while thinking and is flat when idle or in error", () => {
    for (const t of [0, 400, 900]) {
      const a = waveAmplitude("thinking", t);
      expect(a).toBeGreaterThan(0);
      expect(a).toBeLessThan(2);
    }
    expect(waveAmplitude("idle", 100)).toBe(0);
    expect(waveAmplitude("error", 100)).toBe(0);
  });
});

describe("event payloads", () => {
  it("reads a mic level as 0..1 or null", () => {
    expect(levelFromPayload(0.4)).toBe(0.4);
    expect(levelFromPayload(3)).toBe(1);
    expect(levelFromPayload(-1)).toBe(0);
    expect(levelFromPayload(Number.NaN)).toBeNull();
    expect(levelFromPayload("0.4")).toBeNull();
  });
  it("reads the hover flag", () => {
    expect(hoverFromPayload({ on: true })).toBe(true);
    expect(hoverFromPayload({ on: false })).toBe(false);
    expect(hoverFromPayload({ on: 1 })).toBe(false);
    expect(hoverFromPayload(null)).toBe(false);
    expect(hoverFromPayload(true)).toBe(false);
  });
  it("reads the drag flag", () => {
    expect(draggingFromPayload({ dragging: true })).toBe(true);
    expect(draggingFromPayload({ dragging: false })).toBe(false);
    expect(draggingFromPayload({ dragging: "yes" })).toBe(false);
    expect(draggingFromPayload(null)).toBe(false);
    expect(draggingFromPayload(true)).toBe(false);
  });
});

describe("hover controls", () => {
  it("previews hover only with ?hover=1", () => {
    expect(hoverFromSearch("?view=overlay&hover=1")).toBe(true);
    expect(hoverFromSearch("?view=overlay&hover=0")).toBe(false);
    expect(hoverFromSearch("?view=overlay")).toBe(false);
  });
  it("names the mic button by what a click does", () => {
    expect(micLabel("idle")).toBe("Start dictation");
    expect(micLabel("error")).toBe("Start dictation");
    expect(micLabel("listening")).toBe("Stop dictation");
    expect(micLabel("thinking")).toBe("Start dictation");
  });
});
