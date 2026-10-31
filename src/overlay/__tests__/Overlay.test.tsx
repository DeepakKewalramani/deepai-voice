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

describe("Overlay", () => {
  it("starts idle at the notch with no text", () => {
    const m = renderToStaticMarkup(<Overlay />);
    expect(m).toContain('data-mode="idle"');
    expect(m).toContain('data-pos="top"');
    expect(m).toContain('aria-label="DeepAI Voice ready"');
    expect(m).not.toContain("ov-label");
  });
  it("shows a waveform and Listening while recording", () => {
    const m = render("REC");
    expect(m).toContain('data-mode="listening"');
    expect(m).toContain(">Listening<");
    expect(m.match(/<i /g)?.length).toBe(7);
  });
  it("shows three dots and Thinking while transcribing", () => {
    const m = render("...");
    expect(m).toContain('data-mode="thinking"');
    expect(m).toContain(">Thinking<");
    expect(m).toContain("ov-dots");
  });
  it("shows the error text", () => {
    const m = render("Model missing", "left");
    expect(m).toContain('data-mode="error"');
    expect(m).toContain('data-pos="left"');
    expect(m).toContain(">Model missing<");
  });
});
