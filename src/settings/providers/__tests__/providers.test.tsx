import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import vectors from "../validation-vectors.json";
import previewCatalog from "../catalog-preview.json";
import {
  validateBaseUrl,
  validateForm,
  validateHeaderName,
  validateModel,
  validateName,
  authFromSetting,
  emptyForm,
  isLocalUrl,
} from "../validate";
import {
  buildSections,
  maskTail,
  statusChip,
  testLine,
  customProviders,
} from "../model";
import { parseCatalog, parseStatuses, parseTestResult } from "../types";
import { loadProviders, providerApi } from "../useProviders";
import { devPreview } from "../preview";
import {
  INTRO,
  ProvidersHubView,
  ProvidersHubViewProps,
} from "../ProvidersHub";
import { SettingsShell } from "../../SettingsShell";
import { defaultSettings } from "../../types";

const SECRET = "sk-test-SECRET-9f8e7d6c5b4a1234";
const catalog = parseCatalog(previewCatalog);
const noop = () => {};

describe("validation (shared vectors with Rust)", () => {
  it("base URLs", () => {
    for (const [input, want] of vectors.baseUrl.valid)
      expect(validateBaseUrl(input), input).toEqual({ ok: true, value: want });
    for (const input of vectors.baseUrl.invalid) {
      const r = validateBaseUrl(input);
      expect(r.ok, input).toBe(false);
      if (!r.ok && input.length > 12) expect(r.message).not.toContain(input);
    }
  });
  it("names, models and header names", () => {
    for (const s of vectors.name.valid)
      expect(validateName(s).ok, s).toBe(true);
    for (const s of vectors.name.invalid)
      expect(validateName(s).ok, s).toBe(false);
    for (const s of vectors.model.valid)
      expect(validateModel(s).ok, s).toBe(true);
    for (const s of vectors.model.invalid)
      expect(validateModel(s).ok, s).toBe(false);
    for (const s of vectors.headerName.valid)
      expect(validateHeaderName(s).ok, s).toBe(true);
    for (const s of vectors.headerName.invalid)
      expect(validateHeaderName(s).ok, s).toBe(false);
  });
  it("form errors per field, header only for custom auth", () => {
    expect(validateForm(emptyForm)).toEqual({
      name: "Enter a name.",
      baseUrl: expect.any(String),
      model: expect.any(String),
    });
    const good = {
      ...emptyForm,
      name: "Ollama",
      baseUrl: "http://localhost:11434/v1",
      model: "llama3.2",
    };
    expect(validateForm(good)).toEqual({});
    expect(validateForm({ ...good, authStyle: "custom" })).toHaveProperty(
      "headerName",
    );
  });
  it("auth settings round trip into form fields", () => {
    expect(authFromSetting("header:X-API-Key-ID")).toEqual({
      authStyle: "custom",
      headerName: "X-API-Key-ID",
    });
    expect(authFromSetting("none").authStyle).toBe("none");
    expect(authFromSetting(null).authStyle).toBe("bearer");
  });
  it("local URLs", () => {
    expect(isLocalUrl("http://[::1]:11434/v1")).toBe(true);
    expect(isLocalUrl("https://api.example.com")).toBe(false);
  });
});

describe("view model", () => {
  const sarvam = catalog.find((e) => e.id === "sarvam-saaras")!;
  it("status chips", () => {
    expect(
      statusChip({ status: "builtin", setup: "none" }, undefined).label,
    ).toBe("Built in");
    expect(
      statusChip({ status: "candidate", setup: "none" }, undefined).label,
    ).toBe("Candidate");
    expect(statusChip(sarvam, undefined).label).toBe("Not connected");
    expect(
      statusChip({ status: "verified", setup: "custom" }, undefined).label,
    ).toBe("Add your own");
    const [connected] = parseStatuses([
      { id: "sarvam-saaras", connected: true, key: "set", keyTail: "1234" },
    ]);
    expect(statusChip(sarvam, connected)).toEqual({
      label: "Connected",
      tone: "ok",
    });
  });
  it("masks to the last 4 characters only", () => {
    expect(maskTail("1234")).toBe("••••1234");
    expect(maskTail(null)).toBe("Key saved");
    expect(maskTail("12345")).toBe("Key saved");
    const [s] = parseStatuses([{ id: "x", keyTail: SECRET }]);
    expect(s.keyTail).toBeNull();
  });
  it("sections by capability; candidates and builtins cannot connect", () => {
    const sections = buildSections(catalog, []);
    expect(sections.map((s) => s.title)).toEqual([
      "Speech to text",
      "Language model",
      "Voice (TTS)",
    ]);
    const all = sections.flatMap((s) => s.cards);
    const by = (id: string) => all.find((c) => c.entry.id === id)!;
    expect(by("gnani").action).toBe("none");
    expect(by("reverie").action).toBe("none");
    expect(by("whisper-local").action).toBe("none");
    expect(by("sarvam-saaras").action).toBe("connect");
    expect(by("ollama").action).toBe("setup");
    expect(all.some((c) => /tts-1|gpt-4o-mini-tts/.test(c.entry.id))).toBe(
      false,
    );
  });
  it("keeps showing built-in Whisper from settings without a catalog", () => {
    const sections = buildSections([], [], defaultSettings.providers);
    expect(sections[0].cards[0].entry.name).toBe(
      "Local Whisper (whisper-local)",
    );
    expect(sections[0].cards[0].status.label).toBe("Built in");
  });
  it("test result lines", () => {
    expect(testLine({ result: "ok", latencyMs: 12 })).toEqual({
      label: "Connected (12 ms)",
      tone: "ok",
    });
    expect(testLine({ result: "key_rejected" }).tone).toBe("warn");
    expect(testLine({ result: "unreachable" }).tone).toBe("error");
    expect(testLine({ result: "timeout" }).label).toContain("Unreachable");
    expect(parseTestResult({ result: "weird" }).result).toBe("not_available");
    expect(parseTestResult({ result: "other", status: 302 })).toEqual({
      result: "other",
      status: 302,
    });
  });
  it("catalog parser drops bad rows and non-https docs", () => {
    expect(parseCatalog("nope")).toEqual([]);
    const [e] = parseCatalog([
      { id: "a", capability: "stt", docsUrl: "javascript:alert(1)" },
      { id: "b", capability: "video" },
    ]);
    expect(e.docsUrl).toBe("");
    expect(e.status).toBe("candidate");
  });
});

describe("useProviders helpers with mocked invoke", () => {
  it("no backend or a failing backend gives an empty catalog", async () => {
    expect(await loadProviders(undefined)).toEqual({
      catalog: [],
      statuses: [],
    });
    const failing = vi.fn().mockRejectedValue("boom");
    expect(await loadProviders(failing)).toEqual({ catalog: [], statuses: [] });
  });
  it("loads catalog and statuses through the commands", async () => {
    const inv = vi.fn(async (cmd: string) =>
      cmd === "provider_catalog"
        ? previewCatalog
        : [{ id: "anthropic", connected: true, key: "set", keyTail: "abcd" }],
    );
    const r = await loadProviders(inv);
    expect(r.catalog.length).toBe(catalog.length);
    expect(r.statuses[0].keyTail).toBe("abcd");
  });
  it("connect sends the key once and returns only the masked status", async () => {
    const inv = vi.fn().mockResolvedValue({
      id: "anthropic",
      connected: true,
      key: "set",
      keyTail: "1234",
    });
    const r = await providerApi(inv).connect("anthropic", SECRET);
    expect(inv).toHaveBeenCalledWith("connect_provider", {
      id: "anthropic",
      key: SECRET,
    });
    expect(JSON.stringify(r)).not.toContain(SECRET);
  });
  it("errors are shown as plain text, odd errors become a generic line", async () => {
    const api = providerApi(
      vi.fn().mockRejectedValue("That provider is not in the list."),
    );
    expect(await api.disconnect("x")).toEqual({
      ok: false,
      error: "That provider is not in the list.",
    });
    const odd = providerApi(vi.fn().mockRejectedValue(new Error(SECRET)));
    const r = await odd.test("x");
    expect(JSON.stringify(r)).not.toContain(SECRET);
  });
  it("custom save sends the OpenAI-compatible protocol", async () => {
    const inv = vi.fn().mockResolvedValue({ id: "custom-a" });
    await providerApi(inv).saveCustom(emptyForm, "");
    expect(inv.mock.calls[0][1].form.protocol).toBe("openai-compatible");
  });
});

describe("dev preview", () => {
  it("only for the known modes", () => {
    expect(devPreview("")).toBeNull();
    expect(devPreview("?providers=evil")).toBeNull();
    expect(devPreview("?providers=empty")?.statuses).toEqual([]);
    expect(
      customProviders(devPreview("?providers=connected")!.statuses),
    ).toHaveLength(2);
  });
});

const view = (over: Partial<ProvidersHubViewProps> = {}) =>
  renderToStaticMarkup(
    <ProvidersHubView
      catalogCards={buildSections(catalog, [])}
      custom={[]}
      loading={false}
      message={null}
      connecting={null}
      keyDraft=""
      busy={false}
      form={emptyForm}
      formErrors={{}}
      formKey=""
      results={{}}
      confirmRemove={null}
      card={{
        onConnect: noop,
        onKeyChange: noop,
        onSaveKey: noop,
        onCancel: noop,
        onDisconnect: noop,
        onDocs: noop,
        onSetup: noop,
      }}
      formHandlers={{
        onChange: noop,
        onKeyChange: noop,
        onSubmit: noop,
        onCancel: noop,
      }}
      list={{ onTest: noop, onEdit: noop, onRemove: noop }}
      {...over}
    />,
  );

describe("Providers tab markup", () => {
  it("shows the honest intro, three sections and status chips", () => {
    const m = view();
    expect(m).toContain(INTRO);
    expect(m).toContain("Speech to text");
    expect(m).toContain("Language model");
    expect(m).toContain("Voice (TTS)");
    expect(m).toContain("Candidate");
    expect(m).toContain("Built in");
    expect(m).toContain("Not verified yet, so it cannot be connected.");
    expect(m).toContain('role="status"');
    expect(m).not.toContain("\u2014");
  });
  it("inline connect field is a labelled password input", () => {
    const m = view({ connecting: "sarvam-saaras", keyDraft: SECRET });
    expect(m).toContain('for="hub-key-sarvam-saaras"');
    expect(m).toContain('id="hub-key-sarvam-saaras"');
    expect(m).toContain('type="password"');
    expect(m).toContain("Get a key");
  });
  it("connected card shows only the last 4 characters", () => {
    const statuses = parseStatuses([
      { id: "sarvam-saaras", connected: true, key: "set", keyTail: "1234" },
    ]);
    const m = view({ catalogCards: buildSections(catalog, statuses) });
    expect(m).toContain("••••1234");
    expect(m).toContain("Disconnect");
    expect(m).not.toContain(SECRET);
  });
  it("form errors are linked to their inputs", () => {
    const form = { ...emptyForm, baseUrl: "javascript:alert(1)" };
    const m = view({ form, formErrors: validateForm(form) });
    expect(m).toContain('aria-invalid="true"');
    expect(m).toContain('id="hub-c-url-error"');
    expect(m).toContain('aria-describedby="hub-c-url-error"');
  });
  it("custom list shows test results in a live region", () => {
    const custom = parseStatuses([
      {
        id: "custom-a",
        kind: "llm",
        name: "A",
        baseUrl: "https://api.example.com/v1",
        custom: true,
        connected: true,
        key: "set",
        keyTail: "c0de",
      },
    ]);
    const m = view({
      custom,
      results: { "custom-a": { result: "key_rejected" } },
    });
    expect(m).toContain("Test connection");
    expect(m).toContain("Key rejected");
    expect(m).toContain('data-tone="warn"');
    expect(m).toContain('aria-live="polite"');
  });
  it("is mounted in Settings > Providers", () => {
    const m = renderToStaticMarkup(
      <SettingsShell initialSettings={defaultSettings} activeTab="providers" />,
    );
    expect(m).toContain(INTRO);
    expect(m).toContain("Local Whisper");
  });
});
