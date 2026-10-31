import { describe, expect, it } from "vitest";
import {
  IDLE,
  isOverlaySearch,
  overlayStateFromText,
  positionFromText,
  previewFromSearch,
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
    });
  });
});
