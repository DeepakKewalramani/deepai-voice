/** Pure view model for Settings > Providers. */
import type { Provider } from "../types";
import type {
  CatalogEntry,
  Capability,
  ProviderStatus,
  TestResult,
  Tri,
} from "./types";

export type Tone = "ok" | "builtin" | "muted" | "warn" | "error";

export interface Chip {
  label: string;
  tone: Tone;
}

export type CardAction = "connect" | "manage" | "setup" | "none";

export interface CardModel {
  entry: CatalogEntry;
  status: Chip;
  action: CardAction;
  keyTail: string | null;
}

export interface Section {
  capability: Capability;
  title: string;
  cards: CardModel[];
}

export const SECTION_TITLES: Record<Capability, string> = {
  stt: "Speech to text",
  llm: "Language model",
  tts: "Voice (TTS)",
};

/** Status chip: Built in, Connected, Not connected or Candidate. */
export function statusChip(
  entry: Pick<CatalogEntry, "status" | "setup">,
  status: ProviderStatus | undefined,
): Chip {
  if (entry.status === "builtin") return { label: "Built in", tone: "builtin" };
  if (entry.status === "candidate") return { label: "Candidate", tone: "warn" };
  if (entry.setup === "custom") return { label: "Add your own", tone: "muted" };
  if (status?.connected) return { label: "Connected", tone: "ok" };
  return { label: "Not connected", tone: "muted" };
}

const TRI_LABEL: Record<Tri, string> = {
  yes: "Yes",
  no: "No",
  unknown: "Unknown",
};

export const hindiLabel = (t: Tri) => `Hindi: ${TRI_LABEL[t]}`;

export function streamingLabel(t: Tri): string {
  if (t === "yes") return "Live streaming";
  if (t === "no") return "Whole clips";
  return "Streaming: Unknown";
}

/** Masked key: only the last 4 characters, never more. */
export function maskTail(tail: string | null): string {
  return tail && tail.length === 4 ? `••••${tail}` : "Key saved";
}

function actionFor(e: CatalogEntry, s: ProviderStatus | undefined): CardAction {
  if (e.status !== "verified") return "none";
  if (e.setup === "custom") return "setup";
  if (e.setup === "key") return s?.connected ? "manage" : "connect";
  return "none";
}

/**
 * Catalog cards per capability. Built-in providers that are only in settings.json
 * (no catalog loaded, e.g. outside the app) still show, as before.
 */
export function buildSections(
  catalog: CatalogEntry[],
  statuses: ProviderStatus[],
  settingsProviders: Provider[] = [],
): Section[] {
  const byId = new Map(statuses.map((s) => [s.id, s]));
  const entries = [...catalog];
  for (const p of settingsProviders) {
    const known = entries.some((e) => e.id === p.id);
    if (known || p.keyRef || p.id.startsWith("custom-")) continue;
    if (p.kind !== "stt" && p.kind !== "llm" && p.kind !== "tts") continue;
    entries.unshift({
      id: p.id,
      capability: p.kind,
      name: p.id === "whisper-local" ? "Local Whisper (whisper-local)" : p.id,
      vendor: "On this Mac",
      adapter: p.adapter,
      baseUrlDefault: null,
      authStyle: "none",
      defaultModel: p.model,
      languages: `Model ${p.model}`,
      hindi: "unknown",
      streaming: "unknown",
      price: "Free, runs on this Mac",
      docsUrl: "",
      status: "builtin",
      setup: "none",
      note: "Built into the app. Works offline.",
    });
  }
  return (["stt", "llm", "tts"] as const).map((capability) => ({
    capability,
    title: SECTION_TITLES[capability],
    cards: entries
      .filter((e) => e.capability === capability)
      .map((entry) => {
        const s = byId.get(entry.id);
        return {
          entry,
          status: statusChip(entry, s),
          action: actionFor(entry, s),
          keyTail: s?.connected ? s.keyTail : null,
        };
      }),
  }));
}

export const customProviders = (statuses: ProviderStatus[]) =>
  statuses.filter((s) => s.custom);

/** One line for a connection test result. */
export function testLine(r: TestResult): Chip {
  switch (r.result) {
    case "ok":
      return { label: `Connected (${r.latencyMs} ms)`, tone: "ok" };
    case "key_rejected":
      return { label: "Key rejected", tone: "warn" };
    case "unreachable":
      return { label: "Unreachable", tone: "error" };
    case "timeout":
      return { label: "Unreachable: no answer in 10 seconds", tone: "error" };
    case "not_found":
      return {
        label: "Address not found (404). Check the base URL.",
        tone: "error",
      };
    case "other":
      return { label: `Unexpected answer (HTTP ${r.status})`, tone: "error" };
    case "not_available":
      return { label: r.reason, tone: "muted" };
  }
}

export const CAPABILITY_LABEL: Record<Capability, string> = {
  stt: "Speech to text",
  llm: "Language model",
  tts: "Voice (TTS)",
};
