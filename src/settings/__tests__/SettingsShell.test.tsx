import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { SettingsShell } from "../SettingsShell";
import { defaultSettings } from "../types";
import { initialState, isDirty, keyTail, maskKey, reducer } from "../state";

const withKey = {
  ...defaultSettings,
  providers: [
    ...defaultSettings.providers,
    {
      id: "haiku",
      kind: "llm",
      adapter: "anthropic",
      model: "m",
      keyRef: "anthropic",
    },
  ],
};
const render = (tab: Parameters<typeof SettingsShell>[0]["activeTab"]) =>
  renderToStaticMarkup(
    <SettingsShell initialSettings={withKey} activeTab={tab} />,
  );

describe("SettingsShell markup", () => {
  it("renders tablist, tabs and a tabpanel with aria wiring", () => {
    const m = render("general");
    expect(m).toContain('role="tablist"');
    expect(m).toContain('role="tab"');
    expect(m).toContain('aria-selected="true"');
    expect(m).toContain('role="tabpanel"');
    expect(m).toContain('aria-labelledby="tab-general"');
  });
  it("labels point at real controls", () => {
    expect(render("general")).toContain('for="max-duration"');
    expect(render("general")).toContain('<input id="max-duration"');
    expect(render("shortcuts")).toContain('<input id="dictate-key"');
  });
  it("renders each tab", () => {
    expect(render("providers")).toContain("Local Whisper");
    expect(render("styles")).toContain("{{transcript}}");
    expect(render("privacy")).toContain("Local processing by default");
    expect(render("about")).not.toContain("—");
  });
  it("uses a password input for the key and never renders a key", () => {
    const m = render("providers");
    expect(m).toContain('type="password"');
    expect(m).not.toContain("sk-ant-");
  });
  it("shows loading state when loadSettings is given", () => {
    const m = renderToStaticMarkup(
      <SettingsShell loadSettings={() => new Promise(() => {})} />,
    );
    expect(m).toContain("Loading settings");
    expect(m).not.toContain('role="tabpanel"');
  });
  it("disables Save and Reset when clean", () => {
    expect(render("general")).toContain("disabled");
  });
});

describe("settings state", () => {
  const s0 = initialState(defaultSettings, false);
  const edited = reducer(s0, {
    type: "edit",
    settings: { ...defaultSettings, maxRecordSeconds: 30 },
  });
  it("tracks dirty on edit and reset restores initial values", () => {
    expect(isDirty(s0)).toBe(false);
    expect(isDirty(edited)).toBe(true);
    const r = reducer(edited, { type: "reset" });
    expect(r.settings).toEqual(defaultSettings);
    expect(isDirty(r)).toBe(false);
  });
  it("save clears dirty and sets status", () => {
    const s = reducer(edited, {
      type: "saved",
      settings: edited.settings,
      keys: {},
      hints: {},
    });
    expect(isDirty(s)).toBe(false);
    expect(s.status?.text).toBe("Settings saved");
    expect(s.baseline.maxRecordSeconds).toBe(30);
  });
  it("edits typed while saving are kept and stay dirty", () => {
    const later = reducer(edited, {
      type: "edit",
      settings: { ...edited.settings, maxRecordSeconds: 45 },
    });
    const s = reducer(later, {
      type: "saved",
      settings: edited.settings,
      keys: {},
      hints: {},
    });
    expect(s.settings.maxRecordSeconds).toBe(45);
    expect(s.baseline.maxRecordSeconds).toBe(30);
    expect(isDirty(s)).toBe(true);
  });
  it("key draft makes dirty, is cleared on save, only last 4 kept", () => {
    const k = reducer(s0, {
      type: "editKey",
      providerId: "haiku",
      value: "sk-ant-secret1234",
    });
    expect(isDirty(k)).toBe(true);
    const s = reducer(k, {
      type: "saved",
      settings: k.settings,
      keys: { haiku: "sk-ant-secret1234" },
      hints: { haiku: keyTail("sk-ant-secret1234") },
    });
    expect(s.keyDrafts).toEqual({});
    expect(JSON.stringify(s)).not.toContain("secret");
    expect(s.keyHints.haiku).toBe("1234");
    expect(maskKey("1234")).toBe("••••••••1234");
  });
  it("load success and failure", () => {
    const l = initialState(defaultSettings, true);
    expect(reducer(l, { type: "loadFailed", message: "x" }).loadError).toBe(
      "x",
    );
    const ok = reducer(l, {
      type: "loaded",
      settings: { ...defaultSettings, maxRecordSeconds: 5 },
    });
    expect(ok.loading).toBe(false);
    expect(ok.settings.maxRecordSeconds).toBe(5);
  });
  it("short keys reveal no tail", () => {
    expect(keyTail("abc")).toBe("");
  });
});
