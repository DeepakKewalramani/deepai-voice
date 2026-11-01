import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import {
  PERMISSION_KINDS,
  PermissionsStatus,
  UNKNOWN_STATUS,
  buildRows,
  devPreviewStatus,
  missingRequired,
  parseStatus,
  pillFor,
} from "../permissions";
import { loadPermissions } from "../usePermissions";
import { PermissionsList } from "../PermissionsPanel";
import { SettingsShell } from "../SettingsShell";
import { StatusCard } from "../../ui/StatusCard";
import { READY } from "../../ui/status";
import App from "../../App";

const all = (
  s: "granted" | "needed" | "unknown",
  canRequest = false,
): PermissionsStatus => ({
  accessibility: s,
  inputMonitoring: s,
  microphone: s,
  microphoneCanRequest: canRequest,
});

describe("pillFor", () => {
  it("maps each state to a label and tone", () => {
    expect(pillFor("granted")).toEqual({ label: "Granted", tone: "granted" });
    expect(pillFor("needed")).toEqual({ label: "Needed", tone: "needed" });
    expect(pillFor("unknown")).toEqual({ label: "Unknown", tone: "unknown" });
  });
});

describe("kinds", () => {
  it("are the three ids the Rust command accepts, without spaces", () => {
    expect(PERMISSION_KINDS).toEqual([
      "accessibility",
      "inputMonitoring",
      "microphone",
    ]);
    for (const k of PERMISSION_KINDS) expect(k).not.toMatch(/\s/);
  });
});

describe("buildRows", () => {
  it("lists the three rows in order with their copy", () => {
    const rows = buildRows(all("granted"));
    expect(rows.map((r) => r.title)).toEqual([
      "Accessibility",
      "Input Monitoring",
      "Microphone",
    ]);
    expect(rows[0].why).toBe(
      "Lets DeepAI Voice type for you. Without it, text is copied instead.",
    );
    expect(rows[1].why).toBe("Lets the Fn key start dictation.");
    expect(rows[2].why).toBe("Needed to hear you.");
  });
  it("offers Request access only for the mic when not decided", () => {
    expect(buildRows(all("needed", true)).map((r) => r.showRequest)).toEqual([
      false,
      false,
      true,
    ]);
    expect(buildRows(all("needed", false)).some((r) => r.showRequest)).toBe(
      false,
    );
    expect(buildRows(all("granted", true)).some((r) => r.showRequest)).toBe(
      false,
    );
  });
});

describe("missingRequired", () => {
  it("is true only when a permission is known to be needed", () => {
    expect(missingRequired(all("granted"))).toBe(false);
    expect(missingRequired(all("unknown"))).toBe(false);
    expect(missingRequired({ ...all("granted"), microphone: "needed" })).toBe(
      true,
    );
    expect(
      missingRequired({ ...all("unknown"), inputMonitoring: "needed" }),
    ).toBe(true);
  });
});

describe("parseStatus", () => {
  it("accepts a valid payload and falls back to unknown otherwise", () => {
    expect(parseStatus(all("granted", true))).toEqual(all("granted", true));
    expect(parseStatus(null)).toEqual(UNKNOWN_STATUS);
    expect(parseStatus("granted")).toEqual(UNKNOWN_STATUS);
    expect(
      parseStatus({ ...all("granted"), accessibility: "yes", extra: 1 }),
    ).toEqual({ ...all("granted"), accessibility: "unknown" });
  });
});

describe("devPreviewStatus", () => {
  it("reads ?perm= for browser previews", () => {
    expect(devPreviewStatus("?perm=granted")).toEqual(all("granted"));
    expect(devPreviewStatus("?perm=needed")?.microphoneCanRequest).toBe(true);
    expect(devPreviewStatus("?perm=unknown")).toEqual(UNKNOWN_STATUS);
    expect(devPreviewStatus("?perm=mixed")?.accessibility).toBe("needed");
    expect(devPreviewStatus("")).toBeNull();
    expect(devPreviewStatus("?perm=bogus")).toBeNull();
  });
});

describe("loadPermissions", () => {
  it("returns the parsed status from invoke", async () => {
    const invoke = vi.fn().mockResolvedValue(all("granted"));
    await expect(loadPermissions(invoke)).resolves.toEqual(all("granted"));
    expect(invoke).toHaveBeenCalledWith("permissions_status");
  });
  it("returns unknown when invoke fails or is missing", async () => {
    const invoke = vi.fn().mockRejectedValue(new Error("no backend"));
    await expect(loadPermissions(invoke)).resolves.toEqual(UNKNOWN_STATUS);
    await expect(loadPermissions(undefined)).resolves.toEqual(UNKNOWN_STATUS);
  });
});

const list = (status: PermissionsStatus) =>
  renderToStaticMarkup(
    <PermissionsList status={status} onOpen={() => {}} onRequest={() => {}} />,
  );

describe("PermissionsList", () => {
  it("renders every row as a button with a Granted pill", () => {
    const m = list(all("granted"));
    expect(m.match(/<button[^>]*class="perm-row"/g)).toHaveLength(3);
    expect(m.match(/data-tone="granted"/g)).toHaveLength(3);
    expect(m).not.toContain("Speech");
    expect(m).not.toContain("Request access");
    expect(m).toContain("Open Accessibility in System Settings");
  });
  it("shows Needed pills and one Request access button for the mic", () => {
    const m = list(all("needed", true));
    expect(m.match(/data-tone="needed"/g)).toHaveLength(3);
    expect(m.match(/Request access/g)).toHaveLength(1);
  });
  it("shows Unknown pills when the status cannot be read", () => {
    const m = list(UNKNOWN_STATUS);
    expect(m.match(/data-tone="unknown"/g)).toHaveLength(3);
    expect(m.match(/>Unknown</g)).toHaveLength(3);
  });
  it("has no em-dashes", () => {
    expect(list(all("needed", true))).not.toContain("—");
  });
});

describe("SettingsShell permissions tab", () => {
  it("renders the Permissions tab and panel", () => {
    const m = renderToStaticMarkup(<SettingsShell activeTab="permissions" />);
    expect(m).toContain('id="tab-permissions"');
    expect(m).toContain('aria-labelledby="tab-permissions"');
    expect(m).toContain("Input Monitoring");
  });
});

describe("Home permissions hint", () => {
  it("shows a link only when a fix handler is given", () => {
    const withHint = renderToStaticMarkup(
      <StatusCard status={READY} onFixPermissions={() => {}} />,
    );
    expect(withHint).toContain("Permissions needed.");
    expect(withHint).toContain('class="status-perm-link"');
    const without = renderToStaticMarkup(<StatusCard status={READY} />);
    expect(without).not.toContain("Permissions needed");
  });
  it("App lists Permissions as the second Settings tab", () => {
    const m = renderToStaticMarkup(<App initialView="settings" />);
    expect(m.indexOf('id="tab-general"')).toBeLessThan(
      m.indexOf('id="tab-permissions"'),
    );
    expect(m.indexOf('id="tab-permissions"')).toBeLessThan(
      m.indexOf('id="tab-providers"'),
    );
    // Outside Tauri the status is unknown, so Home shows no hint.
    expect(renderToStaticMarkup(<App />)).not.toContain("Permissions needed");
  });
});

describe("Speech Recognition row (optional)", () => {
  const withSpeech = (
    s: "granted" | "needed" | "unknown",
    canRequest: boolean,
  ): PermissionsStatus => ({
    ...all("granted"),
    speechRecognition: s,
    speechCanRequest: canRequest,
  });
  it("is the fourth row when the backend reports it", () => {
    const rows = buildRows(withSpeech("granted", false));
    expect(rows.map((r) => r.kind)).toEqual([
      "accessibility",
      "inputMonitoring",
      "microphone",
      "speechRecognition",
    ]);
    expect(rows[3].title).toBe("Speech Recognition");
    expect(rows[3].showRequest).toBe(false);
  });
  it("offers Request access only while macOS has not asked", () => {
    expect(buildRows(withSpeech("needed", true))[3].showRequest).toBe(true);
    expect(buildRows(withSpeech("needed", false))[3].showRequest).toBe(false);
    const m = list(withSpeech("needed", true));
    expect(m).toContain("Open Speech Recognition in System Settings");
    expect(m.match(/Request access/g)).toHaveLength(1);
    expect(m).not.toContain("—");
  });
  it("is optional: never counts as missing", () => {
    expect(missingRequired(withSpeech("needed", false))).toBe(false);
  });
  it("parses speech fields only when present", () => {
    expect(
      parseStatus({
        ...all("granted"),
        speechRecognition: "needed",
        speechCanRequest: true,
      }),
    ).toEqual(withSpeech("needed", true));
    expect(
      parseStatus({ ...all("granted"), speechRecognition: "bogus" })
        .speechRecognition,
    ).toBe("unknown");
    expect(parseStatus(all("granted"))).not.toHaveProperty("speechRecognition");
  });
});
