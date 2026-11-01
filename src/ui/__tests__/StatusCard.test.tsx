import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { StatusCard } from "../StatusCard";
import {
  DictationStatus,
  LIVE_LINE_MAX,
  READY,
  liveLine,
  statusFromText,
} from "../status";

const render = (status: DictationStatus) =>
  renderToStaticMarkup(<StatusCard status={status} />);

describe("statusFromText", () => {
  it("maps the backend status text to card states", () => {
    expect(statusFromText("")).toEqual(READY);
    expect(statusFromText("REC")).toEqual({ kind: "recording", message: "" });
    expect(statusFromText("...")).toEqual({
      kind: "transcribing",
      message: "",
    });
    expect(statusFromText("Microphone access denied")).toEqual({
      kind: "error",
      message: "Microphone access denied",
    });
  });
});

describe("StatusCard", () => {
  it("shows Ready with Fn as the only advertised key", () => {
    const m = render(READY);
    expect(m).toContain('data-state="ready"');
    expect(m).toContain(">Ready</span>");
    expect(m).toContain("Hold Fn to dictate");
    expect(m).toContain('<kbd class="keycap">Fn</kbd>');
    // The backup shortcut is registered only when Fn is unavailable, so the
    // card must not advertise it.
    expect(m).not.toContain('<kbd class="keycap">Ctrl</kbd>');
  });

  it("shows Recording", () => {
    const m = render({ kind: "recording", message: "" });
    expect(m).toContain('data-state="recording"');
    expect(m).toContain(">Recording</span>");
    expect(m).toContain("Listening");
  });

  it("shows Transcribing", () => {
    const m = render({ kind: "transcribing", message: "" });
    expect(m).toContain('data-state="transcribing"');
    expect(m).toContain(">Transcribing</span>");
  });

  it("shows Error with the backend message", () => {
    const m = render(statusFromText("Copied: paste failed"));
    expect(m).toContain('data-state="error"');
    expect(m).toContain(">Error</span>");
    expect(m).toContain("Copied: paste failed");
  });

  it("announces the state through a live status region", () => {
    const m = render(READY);
    expect(m).toContain('aria-label="Dictation status"');
    expect(m).toContain('role="status" aria-live="polite"');
  });

  it("shows a quiet Live line only while recording", () => {
    const rec = { kind: "recording", message: "" } as const;
    const m = renderToStaticMarkup(
      <StatusCard status={rec} partial="hello there" />,
    );
    expect(m).toContain('class="status-live"');
    expect(m).toContain(">hello there<");
    expect(render(rec)).not.toContain("status-live");
    expect(
      renderToStaticMarkup(<StatusCard status={READY} partial="hello" />),
    ).not.toContain("status-live");
  });
});

describe("liveLine", () => {
  const rec = { kind: "recording", message: "" } as const;
  it("is empty outside recording or without text", () => {
    expect(liveLine(READY, "hi")).toBe("");
    expect(liveLine({ kind: "transcribing", message: "" }, "hi")).toBe("");
    expect(liveLine(rec, "")).toBe("");
    expect(liveLine(rec, undefined)).toBe("");
  });
  it("cleans whitespace and ellipsizes long text at the start", () => {
    expect(liveLine(rec, "  hello \n there ")).toBe("hello there");
    const t = liveLine(rec, "word ".repeat(60) + "last");
    expect(Array.from(t).length).toBeLessThanOrEqual(LIVE_LINE_MAX);
    expect(t.startsWith("\u2026")).toBe(true);
    expect(t.endsWith("word last")).toBe(true);
  });
});
