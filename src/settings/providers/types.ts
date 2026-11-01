/** Shapes sent by the Rust provider commands (src-tauri/src/providers). */

export type Capability = "stt" | "llm" | "tts";
export type Tri = "yes" | "no" | "unknown";
export type EntryStatus = "builtin" | "verified" | "candidate";
export type Setup = "none" | "key" | "custom";

export interface CatalogEntry {
  id: string;
  capability: Capability;
  name: string;
  vendor: string;
  adapter: string;
  baseUrlDefault: string | null;
  /** `bearer`, `token`, `x-api-key`, `none`, `unknown` or `header:<name>`. */
  authStyle: string;
  defaultModel: string;
  languages: string;
  hindi: Tri;
  streaming: Tri;
  price: string;
  docsUrl: string;
  status: EntryStatus;
  setup: Setup;
  note: string;
}

export type KeyState = "set" | "not_set" | "not_needed";

/** A configured provider. Never carries a key, only its last 4 characters. */
export interface ProviderStatus {
  id: string;
  kind: string;
  adapter: string;
  model: string;
  name: string | null;
  baseUrl: string | null;
  authStyle: string | null;
  custom: boolean;
  connected: boolean;
  key: KeyState;
  keyTail: string | null;
}

export type TestResult =
  | { result: "not_available"; reason: string }
  | { result: "ok"; latencyMs: number }
  | { result: "key_rejected" }
  | { result: "not_found" }
  | { result: "unreachable" }
  | { result: "timeout" }
  | { result: "other"; status: number };

const CAPS: readonly string[] = ["stt", "llm", "tts"];
const TRIS: readonly string[] = ["yes", "no", "unknown"];
const STATUSES: readonly string[] = ["builtin", "verified", "candidate"];
const SETUPS: readonly string[] = ["none", "key", "custom"];
const KEYS: readonly string[] = ["set", "not_set", "not_needed"];

const str = (v: unknown): string => (typeof v === "string" ? v : "");
const strOrNull = (v: unknown): string | null =>
  typeof v === "string" ? v : null;
const oneOf = <T extends string>(
  v: unknown,
  allowed: readonly string[],
  fallback: T,
): T => (typeof v === "string" && allowed.includes(v) ? (v as T) : fallback);

/** Validates the catalog payload; drops malformed rows. */
export function parseCatalog(raw: unknown): CatalogEntry[] {
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((r): CatalogEntry[] => {
    if (typeof r !== "object" || r === null) return [];
    const o = r as Record<string, unknown>;
    if (!str(o.id) || !CAPS.includes(str(o.capability))) return [];
    const docs = str(o.docsUrl);
    return [
      {
        id: str(o.id),
        capability: o.capability as Capability,
        name: str(o.name),
        vendor: str(o.vendor),
        adapter: str(o.adapter),
        baseUrlDefault: strOrNull(o.baseUrlDefault),
        authStyle: str(o.authStyle),
        defaultModel: str(o.defaultModel),
        languages: str(o.languages),
        hindi: oneOf(o.hindi, TRIS, "unknown"),
        streaming: oneOf(o.streaming, TRIS, "unknown"),
        price: str(o.price),
        docsUrl: docs.startsWith("https://") ? docs : "",
        // Anything unexpected is treated as not connectable.
        status: oneOf(o.status, STATUSES, "candidate"),
        setup: oneOf(o.setup, SETUPS, "none"),
        note: str(o.note),
      },
    ];
  });
}

export function parseStatuses(raw: unknown): ProviderStatus[] {
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((r): ProviderStatus[] => {
    if (typeof r !== "object" || r === null) return [];
    const o = r as Record<string, unknown>;
    if (!str(o.id)) return [];
    return [parseStatus(o)];
  });
}

export function parseStatus(o: Record<string, unknown>): ProviderStatus {
  const tail = strOrNull(o.keyTail);
  return {
    id: str(o.id),
    kind: str(o.kind),
    adapter: str(o.adapter),
    model: str(o.model),
    name: strOrNull(o.name),
    baseUrl: strOrNull(o.baseUrl),
    authStyle: strOrNull(o.authStyle),
    custom: o.custom === true,
    connected: o.connected === true,
    key: oneOf(o.key, KEYS, "not_set"),
    // Only ever a 4-character tail.
    keyTail: tail !== null && tail.length === 4 ? tail : null,
  };
}

export function parseTestResult(raw: unknown): TestResult {
  const o = (typeof raw === "object" && raw !== null ? raw : {}) as Record<
    string,
    unknown
  >;
  switch (o.result) {
    case "ok":
      return {
        result: "ok",
        latencyMs: typeof o.latencyMs === "number" ? o.latencyMs : 0,
      };
    case "key_rejected":
    case "not_found":
    case "unreachable":
    case "timeout":
      return { result: o.result };
    case "other":
      return {
        result: "other",
        status: typeof o.status === "number" ? o.status : 0,
      };
    default:
      return {
        result: "not_available",
        reason: str(o.reason) || "The test could not run.",
      };
  }
}
