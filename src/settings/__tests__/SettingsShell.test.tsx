import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { SettingsShell } from "../SettingsShell";
import { defaultSettings } from "../types";

describe("SettingsShell", () => {
  it("renders the settings heading and navigation tabs", () => {
    const markup = renderToStaticMarkup(
      <SettingsShell initialSettings={defaultSettings} activeTab="general" />,
    );
    expect(markup).toContain("Settings");
    expect(markup).toContain("General");
    expect(markup).toContain("Providers");
    expect(markup).toContain("Shortcuts");
    expect(markup).toContain("Styles");
    expect(markup).toContain("Privacy");
    expect(markup).toContain("About");
  });

  it("renders General tab content by default", () => {
    const markup = renderToStaticMarkup(
      <SettingsShell initialSettings={defaultSettings} activeTab="general" />,
    );
    expect(markup).toContain("Maximum recording duration");
    expect(markup).toContain("120 seconds");
  });

  it("renders Providers tab content when active", () => {
    const markup = renderToStaticMarkup(
      <SettingsShell initialSettings={defaultSettings} activeTab="providers" />,
    );
    expect(markup).toContain("Speech-to-Text Providers");
    expect(markup).toContain("whisper-local");
    expect(markup).toContain("Local Whisper");
  });

  it("renders Shortcuts tab content when active", () => {
    const markup = renderToStaticMarkup(
      <SettingsShell initialSettings={defaultSettings} activeTab="shortcuts" />,
    );
    expect(markup).toContain("Dictation shortcut");
    expect(markup).toContain("Ctrl+Alt+Space");
  });

  it("renders Styles tab content with transcript tag validation", () => {
    const markup = renderToStaticMarkup(
      <SettingsShell initialSettings={defaultSettings} activeTab="styles" />,
    );
    expect(markup).toContain("Clean up");
    expect(markup).toContain("{{transcript}}");
  });

  it("renders Privacy tab content with local data notice", () => {
    const markup = renderToStaticMarkup(
      <SettingsShell initialSettings={defaultSettings} activeTab="privacy" />,
    );
    expect(markup).toContain("Privacy &amp; Data Safety");
    expect(markup).toContain("Local processing by default");
  });

  it("never displays raw API keys in plain text", () => {
    const settingsWithKey = {
      ...defaultSettings,
      providers: [
        ...defaultSettings.providers,
        {
          id: "test-haiku",
          kind: "llm",
          adapter: "anthropic",
          model: "claude-haiku-5-5",
          keyRef: "anthropic",
        },
      ],
    };
    const markup = renderToStaticMarkup(
      <SettingsShell initialSettings={settingsWithKey} activeTab="providers" />,
    );
    // Keys must be masked
    expect(markup).not.toContain("sk-ant-");
    expect(markup).toContain("••••••••");
  });
});
