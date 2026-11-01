/**
 * Dev browser preview only (`npm run dev`, outside Tauri): `?providers=connected`,
 * `?providers=empty` or `?providers=error` shows the Providers tab with the bundled
 * catalog copy and fake statuses. Never used in the app or in a production build.
 * The copy is kept equal to the Rust catalog by `provider_catalog_test.rs`.
 */
import previewCatalog from "./catalog-preview.json";
import {
  CatalogEntry,
  ProviderStatus,
  TestResult,
  parseCatalog,
} from "./types";
import { CustomForm, emptyForm } from "./validate";

export interface ProvidersPreview {
  catalog: CatalogEntry[];
  statuses: ProviderStatus[];
  form?: CustomForm;
  /** Opens the inline key field on this catalog id. */
  connecting?: string;
  results?: Record<string, TestResult>;
}

const status = (
  over: Partial<ProviderStatus> & Pick<ProviderStatus, "id" | "kind">,
): ProviderStatus => ({
  adapter: "",
  model: "",
  name: null,
  baseUrl: null,
  authStyle: null,
  custom: false,
  connected: true,
  key: "set",
  keyTail: "7f3a",
  ...over,
});

export function devPreview(search: string): ProvidersPreview | null {
  const mode = new URLSearchParams(search).get("providers");
  if (mode !== "connected" && mode !== "empty" && mode !== "error") return null;
  const catalog = parseCatalog(previewCatalog);
  if (mode === "empty") return { catalog, statuses: [] };
  if (mode === "error")
    return {
      catalog,
      statuses: [],
      connecting: "deepgram-nova-3",
      form: {
        ...emptyForm,
        name: "My server",
        baseUrl: "http://192.168.1.10/v1?key=abc",
        model: "two words",
      },
    };
  return {
    catalog,
    results: {
      "custom-ollama": { result: "ok", latencyMs: 38 },
      "custom-my-gateway": { result: "key_rejected" },
    },
    statuses: [
      status({ id: "sarvam-saaras", kind: "stt", keyTail: "7f3a" }),
      status({ id: "anthropic", kind: "llm", keyTail: "x9Qe" }),
      status({
        id: "custom-ollama",
        kind: "llm",
        adapter: "openai_compatible",
        model: "llama3.2",
        name: "Ollama",
        baseUrl: "http://localhost:11434/v1",
        authStyle: "none",
        custom: true,
        key: "not_needed",
        keyTail: null,
      }),
      status({
        id: "custom-my-gateway",
        kind: "llm",
        adapter: "openai_compatible",
        model: "gpt-5-nano",
        name: "My gateway",
        baseUrl: "https://api.example.com/v1",
        authStyle: "bearer",
        custom: true,
        keyTail: "c0de",
      }),
    ],
  };
}
