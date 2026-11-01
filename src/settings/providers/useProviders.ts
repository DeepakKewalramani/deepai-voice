import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  CatalogEntry,
  ProviderStatus,
  TestResult,
  parseCatalog,
  parseStatus,
  parseStatuses,
  parseTestResult,
} from "./types";
import type { CustomForm } from "./validate";
import { devPreview } from "./preview";

export type Invoke = (
  cmd: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;

export type Outcome<T> = { ok: true; value: T } | { ok: false; error: string };

export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Rust errors are fixed sentences; anything else gets a generic line. */
function errorText(e: unknown): string {
  return typeof e === "string" && e.length > 0 && e.length < 300
    ? e
    : "Something went wrong. Try again.";
}

async function call<T>(
  invokeFn: Invoke | undefined,
  cmd: string,
  args: Record<string, unknown> | undefined,
  parse: (raw: unknown) => T,
): Promise<Outcome<T>> {
  if (!invokeFn) return { ok: false, error: "Available in the app only." };
  try {
    return { ok: true, value: parse(await invokeFn(cmd, args)) };
  } catch (e) {
    return { ok: false, error: errorText(e) };
  }
}

const statusOf = (raw: unknown) =>
  parseStatus((raw ?? {}) as Record<string, unknown>);

/** Loads catalog and statuses. Missing or failing backend reads empty. */
export async function loadProviders(
  invokeFn: Invoke | undefined,
): Promise<{ catalog: CatalogEntry[]; statuses: ProviderStatus[] }> {
  const [c, s] = await Promise.all([
    call(invokeFn, "provider_catalog", undefined, parseCatalog),
    call(invokeFn, "providers_status", undefined, parseStatuses),
  ]);
  return {
    catalog: c.ok ? c.value : [],
    statuses: s.ok ? s.value : [],
  };
}

/** Command wrappers. Keys are passed through once and never kept. */
export function providerApi(invokeFn: Invoke | undefined) {
  return {
    connect: (id: string, key: string) =>
      call(invokeFn, "connect_provider", { id, key }, statusOf),
    disconnect: (id: string) =>
      call(invokeFn, "disconnect_provider", { id }, () => undefined),
    saveCustom: (form: CustomForm, key: string) =>
      call(
        invokeFn,
        "save_custom_provider",
        {
          form: { ...form, protocol: "openai-compatible" },
          key,
        },
        statusOf,
      ),
    deleteCustom: (id: string) =>
      call(invokeFn, "delete_custom_provider", { id }, () => undefined),
    test: (id: string) =>
      call(invokeFn, "test_provider", { id }, parseTestResult),
    openDocs: (id: string) =>
      call(invokeFn, "open_provider_docs", { id }, () => undefined),
  };
}

export type ProviderApi = ReturnType<typeof providerApi>;

export interface UseProviders {
  catalog: CatalogEntry[];
  statuses: ProviderStatus[];
  loading: boolean;
  refresh: () => void;
  api: ProviderApi;
  /** Dev browser preview state, if any (`?providers=`). */
  preview: ReturnType<typeof devPreview>;
}

function initialPreview() {
  if (import.meta.env.DEV && typeof window !== "undefined" && !inTauri())
    return devPreview(window.location.search);
  return null;
}

export function useProviders(): UseProviders {
  const [preview] = useState(initialPreview);
  const [catalog, setCatalog] = useState<CatalogEntry[]>(
    preview?.catalog ?? [],
  );
  const [statuses, setStatuses] = useState<ProviderStatus[]>(
    preview?.statuses ?? [],
  );
  const [loading, setLoading] = useState(inTauri);
  const [api] = useState(() => providerApi(inTauri() ? invoke : undefined));

  const refresh = useCallback(() => {
    if (!inTauri()) return;
    void loadProviders(invoke).then((r) => {
      setCatalog(r.catalog);
      setStatuses(r.statuses);
      setLoading(false);
    });
  }, []);

  useEffect(refresh, [refresh]);

  return { catalog, statuses, loading, refresh, api, preview };
}

export type { TestResult };
