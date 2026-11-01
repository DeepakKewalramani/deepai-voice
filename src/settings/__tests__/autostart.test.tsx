import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import {
  AutostartState,
  autostartReducer,
  initialAutostart,
  loadAutostart,
  saveAutostart,
} from "../autostart";
import { LaunchAtLoginRow } from "../LaunchAtLogin";
import { SettingsShell } from "../SettingsShell";

describe("autostartReducer", () => {
  it("loads, toggles optimistically and rolls back on failure", () => {
    let s: AutostartState = initialAutostart;
    expect(s.enabled).toBeNull();
    s = autostartReducer(s, { type: "loaded", enabled: true });
    expect(s).toEqual({ enabled: true, busy: false, error: null });
    s = autostartReducer(s, { type: "toggle", enabled: false });
    expect(s).toEqual({ enabled: false, busy: true, error: null });
    s = autostartReducer(s, {
      type: "failed",
      previous: true,
      message: "nope",
    });
    expect(s).toEqual({ enabled: true, busy: false, error: "nope" });
    s = autostartReducer(s, { type: "toggle", enabled: false });
    expect(s.error).toBeNull();
    s = autostartReducer(s, { type: "saved" });
    expect(s).toEqual({ enabled: false, busy: false, error: null });
  });
});

describe("loadAutostart / saveAutostart", () => {
  it("reads the status and treats a missing backend as off", async () => {
    const invoke = vi.fn().mockResolvedValue(true);
    await expect(loadAutostart(invoke)).resolves.toBe(true);
    expect(invoke).toHaveBeenCalledWith("autostart_status");
    await expect(loadAutostart(undefined)).resolves.toBe(false);
    await expect(
      loadAutostart(vi.fn().mockRejectedValue(new Error("x"))),
    ).resolves.toBe(false);
    await expect(loadAutostart(vi.fn().mockResolvedValue("yes"))).resolves.toBe(
      false,
    );
  });
  it("returns null on success and the backend message on failure", async () => {
    const ok = vi.fn().mockResolvedValue(undefined);
    await expect(saveAutostart(ok, true)).resolves.toBeNull();
    expect(ok).toHaveBeenCalledWith("set_autostart", { enabled: true });
    await expect(
      saveAutostart(
        vi.fn().mockRejectedValue("Only in the installed app."),
        true,
      ),
    ).resolves.toBe("Only in the installed app.");
    await expect(saveAutostart(undefined, true)).resolves.toBe(
      "Launch at login is available only in the installed app.",
    );
  });
});

const row = (s: Partial<AutostartState>) =>
  renderToStaticMarkup(
    <LaunchAtLoginRow
      state={{ ...initialAutostart, ...s }}
      onToggle={() => {}}
    />,
  );

describe("LaunchAtLoginRow", () => {
  it("is a labelled switch with help text", () => {
    const m = row({ enabled: true });
    expect(m).toContain('role="switch"');
    expect(m).toContain('aria-checked="true"');
    expect(m).toContain("Launch at login");
    expect(m).toContain(
      "Starts DeepAI Voice when you log in, so you never open it by hand.",
    );
    expect(m).not.toContain('role="alert"');
  });
  it("shows off, disabled while unknown, and the error line", () => {
    expect(row({ enabled: false })).toContain('aria-checked="false"');
    expect(row({ enabled: null })).toContain("disabled");
    const m = row({ enabled: false, error: "Not in a dev build." });
    expect(m).toContain('role="alert"');
    expect(m).toContain("Not in a dev build.");
  });
});

describe("General tab", () => {
  it("starts with the Launch at login row", () => {
    const m = renderToStaticMarkup(<SettingsShell activeTab="general" />);
    expect(m).toContain('role="switch"');
    expect(m.indexOf("Launch at login")).toBeLessThan(
      m.indexOf("Maximum recording duration"),
    );
  });
});
