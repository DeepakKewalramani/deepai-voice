import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import App from "./App";

describe("App", () => {
  it("renders the product name as the heading", () => {
    expect(renderToStaticMarkup(<App />)).toContain("<h1>DeepAI Voice</h1>");
  });

  it("has a labelled main navigation with the current view marked", () => {
    const m = renderToStaticMarkup(<App />);
    expect(m).toContain('<nav class="nav" aria-label="Main">');
    for (const label of ["Home", "Dictionary", "Styles", "Settings"])
      expect(m).toContain(`<span class="nav-label">${label}</span>`);
    expect(m.match(/aria-current="page"/g)).toHaveLength(1);
    expect(m).toMatch(/aria-current="page"[^>]*title="Home"/);
  });

  it("opens on Home with the Ready status card, steps and privacy note", () => {
    const m = renderToStaticMarkup(<App />);
    expect(m).toContain('data-state="ready"');
    expect(m).toContain("How it works");
    expect(m).toContain("Your dictations are not stored");
    expect(m).not.toContain("History");
  });

  it("shows the Dictionary page, loading until the list arrives", () => {
    const m = renderToStaticMarkup(<App initialView="dictionary" />);
    expect(m).not.toContain("Coming soon");
    expect(m).toContain("Stored only on this Mac");
    expect(m).toContain("Loading your dictionary...");
    expect(m).toMatch(/aria-current="page"[^>]*title="Dictionary"/);
  });

  it("shows Styles without a tab bar and Settings with the other tabs", () => {
    const styles = renderToStaticMarkup(<App initialView="styles" />);
    expect(styles).toContain('for="active-style"');
    expect(styles).not.toContain('role="tablist"');

    const settings = renderToStaticMarkup(<App initialView="settings" />);
    expect(settings).toContain('role="tablist"');
    for (const id of ["general", "providers", "shortcuts", "privacy", "about"])
      expect(settings).toContain(`id="tab-${id}"`);
    expect(settings).not.toContain('id="tab-styles"');
  });
});
