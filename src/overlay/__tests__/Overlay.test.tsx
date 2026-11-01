import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import Overlay from "../Overlay";
import { overlayStateFromText } from "../state";

const render = (
  status: string,
  pos: "top" | "bottom" | "left" | "right" = "top",
) =>
  renderToStaticMarkup(
    <Overlay
      initialState={overlayStateFromText(status)}
      initialPosition={pos}
    />,
  );

/** Text between tags, so no state shows words inside the capsule. */
const visibleText = (m: string) => m.replace(/<[^>]*>/g, "").trim();

describe("Overlay", () => {
  it("is a plain empty capsule when idle", () => {
    const m = renderToStaticMarkup(<Overlay />);
    expect(m).toContain('data-mode="idle"');
    expect(m).toContain('data-pos="top"');
    expect(m).toContain('aria-label="DeepAI Voice ready"');
    expect(m).toContain("--w:76px");
    expect(m).toContain("--h:24px");
    // Nothing inside: no wave, no orb, no dots.
    expect(m).not.toContain("<svg");
    expect(m).not.toContain("ov-orb");
    expect(m).not.toContain("ov-eq");
    expect(visibleText(m)).toBe("");
  });
  it("shows one gradient sine line while listening", () => {
    const m = render("REC");
    expect(m).toContain('data-mode="listening"');
    expect(m).toContain('aria-label="Listening"');
    expect(m).toContain("--w:116px");
    expect(m).toContain("--h:26px");
    expect(m.match(/<path/g)?.length).toBe(1);
    expect(m).toContain("linearGradient");
    expect(m).toContain("#ff8a4c");
    expect(m).toContain("#b86bff");
    expect(visibleText(m)).toBe("");
  });
  it("settles into a flat line while thinking", () => {
    const m = render("...");
    expect(m).toContain('data-mode="thinking"');
    expect(m).toContain('aria-label="Thinking"');
    expect(m.match(/<path/g)?.length).toBe(1);
    expect(visibleText(m)).toBe("");
  });
  it("never shows live text", () => {
    const m = renderToStaticMarkup(
      <Overlay
        initialState={overlayStateFromText("REC")}
        initialPartial="hello there"
      />,
    );
    expect(m).not.toContain("hello there");
    expect(m).toContain("--w:116px");
  });
  it("shows an error as a small red line with no text", () => {
    const m = render("Model missing", "left");
    expect(m).toContain('data-mode="error"');
    expect(m).toContain('data-pos="left"');
    expect(m).toContain("--w:76px");
    expect(m.match(/<path/g)?.length).toBe(1);
    expect(visibleText(m)).toBe("");
    // The message stays available to assistive tech only.
    expect(m).toContain('aria-label="Model missing"');
  });
  it("stands upright on the left and right anchors only", () => {
    for (const pos of ["left", "right"] as const) {
      const m = render("REC", pos);
      expect(m).toContain(`data-pos="${pos}"`);
      expect(m).toContain('data-orient="vertical"');
      // Same unrotated size and one wave: CSS turns the whole capsule.
      expect(m).toContain("--w:116px");
      expect(m.match(/<path/g)?.length).toBe(1);
    }
    for (const pos of ["top", "bottom"] as const) {
      expect(render("", pos)).toContain('data-orient="horizontal"');
    }
  });
  it("marks the lifted look while dragging", () => {
    expect(render("")).not.toContain("data-dragging");
    const m = renderToStaticMarkup(<Overlay initialDragging />);
    expect(m).toContain('data-dragging="true"');
  });
});
