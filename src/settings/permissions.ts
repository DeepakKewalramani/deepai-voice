/** State of one macOS permission, as sent by the `permissions_status` command. */
export type PermissionState = "granted" | "needed" | "unknown";

/** Permission ids, the same strings the Rust `PermissionKind::from_id` accepts. */
export const PERMISSION_KINDS = [
  "accessibility",
  "inputMonitoring",
  "microphone",
] as const;
export type PermissionKind = (typeof PERMISSION_KINDS)[number];

export type PermissionsStatus = Record<PermissionKind, PermissionState> & {
  /** True while macOS has not asked yet, so a prompt can be shown. */
  microphoneCanRequest: boolean;
};

export const UNKNOWN_STATUS: PermissionsStatus = {
  accessibility: "unknown",
  inputMonitoring: "unknown",
  microphone: "unknown",
  microphoneCanRequest: false,
};

const copy: Record<PermissionKind, { title: string; why: string }> = {
  accessibility: {
    title: "Accessibility",
    why: "Lets DeepAI Voice type for you. Without it, text is copied instead.",
  },
  inputMonitoring: {
    title: "Input Monitoring",
    why: "Lets the Fn key start dictation.",
  },
  microphone: {
    title: "Microphone",
    why: "Needed to hear you.",
  },
};

export type PillTone = PermissionState;

const pillLabels: Record<PermissionState, string> = {
  granted: "Granted",
  needed: "Needed",
  unknown: "Unknown",
};

/** Status pill for a row: Granted green, Needed amber, Unknown grey. */
export function pillFor(state: PermissionState): {
  label: string;
  tone: PillTone;
} {
  return { label: pillLabels[state], tone: state };
}

export interface PermissionRow {
  kind: PermissionKind;
  title: string;
  why: string;
  pill: { label: string; tone: PillTone };
  /** Show the secondary "Request access" button. */
  showRequest: boolean;
}

export function buildRows(status: PermissionsStatus): PermissionRow[] {
  return PERMISSION_KINDS.map((kind) => ({
    kind,
    ...copy[kind],
    pill: pillFor(status[kind]),
    showRequest:
      kind === "microphone" &&
      status.microphone !== "granted" &&
      status.microphoneCanRequest,
  }));
}

/** True when any permission is known to be missing. */
export function missingRequired(status: PermissionsStatus): boolean {
  return PERMISSION_KINDS.some((k) => status[k] === "needed");
}

const STATES: readonly string[] = ["granted", "needed", "unknown"];

/** Validates a backend payload; anything unexpected reads as unknown. */
export function parseStatus(raw: unknown): PermissionsStatus {
  if (typeof raw !== "object" || raw === null) return UNKNOWN_STATUS;
  const r = raw as Record<string, unknown>;
  const state = (v: unknown): PermissionState =>
    typeof v === "string" && STATES.includes(v)
      ? (v as PermissionState)
      : "unknown";
  return {
    accessibility: state(r.accessibility),
    inputMonitoring: state(r.inputMonitoring),
    microphone: state(r.microphone),
    microphoneCanRequest: r.microphoneCanRequest === true,
  };
}

/**
 * Dev browser preview only: `?perm=granted|needed|unknown|mixed` forces a
 * status. Null when the param is absent or not one of those.
 */
export function devPreviewStatus(search: string): PermissionsStatus | null {
  const p = new URLSearchParams(search).get("perm");
  const every = (s: PermissionState, req: boolean): PermissionsStatus => ({
    accessibility: s,
    inputMonitoring: s,
    microphone: s,
    microphoneCanRequest: req,
  });
  switch (p) {
    case "granted":
      return every("granted", false);
    case "needed":
      return every("needed", true);
    case "unknown":
      return UNKNOWN_STATUS;
    case "mixed":
      return {
        ...every("granted", false),
        accessibility: "needed",
      };
    default:
      return null;
  }
}
