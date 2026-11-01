import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DictionaryApi, Invoke, Outcome, dictionaryApi } from "./api";
import { AddManyReport, DictEntry, EntryInput } from "./model";
import { devPreviewInvoke } from "./preview";

export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Real backend in the app; the in-memory fake only in the dev browser. */
function pickBackend(): Invoke | undefined {
  if (inTauri()) return invoke;
  if (import.meta.env.DEV && typeof window !== "undefined")
    return devPreviewInvoke(window.location.search);
  return undefined;
}

export type LoadState = "loading" | "ready" | "error";

export interface UseDictionary {
  entries: DictEntry[];
  state: LoadState;
  /** Fixed sentence when `state` is "error". */
  error: string;
  reload: () => void;
  add: (input: EntryInput) => Promise<Outcome<DictEntry>>;
  update: (id: string, input: EntryInput) => Promise<Outcome<DictEntry>>;
  remove: (id: string) => Promise<Outcome<void>>;
  addMany: (text: string, language: string) => Promise<Outcome<AddManyReport>>;
}

/**
 * Loads the dictionary and keeps the list in step with each command. Every
 * call goes through `dictionaryApi`, which catches errors.
 */
export function useDictionary(backend?: Invoke) {
  const [api] = useState<DictionaryApi>(() =>
    dictionaryApi(backend ?? pickBackend()),
  );
  const [entries, setEntries] = useState<DictEntry[]>([]);
  const [state, setState] = useState<LoadState>("loading");
  const [error, setError] = useState("");

  const load = useCallback(() => {
    void api.list().then((r) => {
      if (r.ok) {
        setEntries(r.value);
        setState("ready");
      } else {
        setError(r.error);
        setState("error");
      }
    });
  }, [api]);

  useEffect(load, [load]);

  const reload = useCallback(() => {
    setState("loading");
    load();
  }, [load]);

  const add = useCallback(
    async (input: EntryInput) => {
      const r = await api.add(input);
      if (r.ok) setEntries((list) => [...list, r.value]);
      return r;
    },
    [api],
  );

  const update = useCallback(
    async (id: string, input: EntryInput) => {
      const r = await api.update(id, input);
      if (r.ok)
        setEntries((list) => list.map((e) => (e.id === id ? r.value : e)));
      return r;
    },
    [api],
  );

  const remove = useCallback(
    async (id: string) => {
      const r = await api.remove(id);
      if (r.ok) setEntries((list) => list.filter((e) => e.id !== id));
      return r;
    },
    [api],
  );

  const addMany = useCallback(
    async (text: string, language: string) => {
      const r = await api.addMany(text, language);
      // The backend decides what was added: read the list again.
      if (r.ok) {
        const fresh = await api.list();
        if (fresh.ok) setEntries(fresh.value);
      }
      return r;
    },
    [api],
  );

  return {
    entries,
    state,
    error,
    reload,
    add,
    update,
    remove,
    addMany,
  } satisfies UseDictionary;
}
