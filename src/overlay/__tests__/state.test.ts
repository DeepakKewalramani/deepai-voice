import { describe, expect, it } from "vitest";
import {
  CAPSULE_SIZES,
  IDLE,
  WAVE_H,
  WAVE_W,
  capsuleSize,
  draggingFromPayload,
  isOverlaySearch,
  levelFromPayload,
  overlayStateFromText,
  positionFromText,
  previewFromSearch,
  sinePath,
  waveAmplitude,
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

describe("capsule sizes", () => {
  it("is a small plain capsule when idle or in error", () => {
    expect(capsuleSize("idle")).toEqual({ width: 60, height: 30 });
    expect(capsuleSize("error")).toEqual(capsuleSize("idle"));
  });
  it("grows to hold the wave while listening or thinking", () => {
    expect(capsuleSize("listening")).toEqual({ width: 92, height: 34 });
    expect(capsuleSize("thinking")).toEqual(capsuleSize("listening"));
  });
  it("fits every size in the 120 x 56 overlay window with the drag lift", () => {
    for (const { width, height } of Object.values(CAPSULE_SIZES)) {
      expect(width * 1.04 + 16).toBeLessThanOrEqual(120);
      expect(height * 1.04 + 16).toBeLessThanOrEqual(56);
    }
  });
});

describe("sine wave", () => {
  it("spans the wave box and keeps both ends on the midline", () => {
    const d = sinePath(1234, 7);
    expect(d.startsWith("M0 ")).toBe(true);
    expect(d).toContain(`L${WAVE_W} `);
    const y = ys(d);
    expect(y[0]).toBeCloseTo(WAVE_H / 2, 5);
    expect(y[y.length - 1]).toBeCloseTo(WAVE_H / 2, 5);
    for (const v of y) {
      expect(v).toBeGreaterThanOrEqual(WAVE_H / 2 - 7);
      expect(v).toBeLessThanOrEqual(WAVE_H / 2 + 7);
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
    expect(Math.min(...amps)).toBeGreaterThanOrEqual(7 * 0.2);
    expect(Math.max(...amps)).toBeLessThanOrEqual(7);
    expect(Math.max(...amps) - Math.min(...amps)).toBeGreaterThan(1);
    expect(waveAmplitude("listening", 0, 1)).toBe(7);
    expect(waveAmplitude("listening", 0, 0)).toBeCloseTo(1.75);
    expect(waveAmplitude("listening", 0, 5)).toBe(7);
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
  it("reads the drag flag", () => {
    expect(draggingFromPayload({ dragging: true })).toBe(true);
    expect(draggingFromPayload({ dragging: false })).toBe(false);
    expect(draggingFromPayload({ dragging: "yes" })).toBe(false);
    expect(draggingFromPayload(null)).toBe(false);
    expect(draggingFromPayload(true)).toBe(false);
  });
});
