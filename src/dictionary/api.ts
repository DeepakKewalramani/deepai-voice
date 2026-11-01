/** Typed wrappers over the Rust `dictionary_*` commands. Never throws. */
import {
  AddManyReport,
  DictEntry,
  EntryInput,
  parseEntries,
  parseEntry,
  parseReport,
} from "./model";

export type Invoke = (
  cmd: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;

export type Outcome<T> = { ok: true; value: T } | { ok: false; error: string };

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

function entryOrThrow(raw: unknown): DictEntry {
  const e = parseEntry(raw);
  if (!e) throw new Error("bad entry");
  return e;
}

export function dictionaryApi(invokeFn: Invoke | undefined) {
  return {
    list: () => call(invokeFn, "dictionary_list", undefined, parseEntries),
    add: (entry: EntryInput) =>
      call(invokeFn, "dictionary_add", { entry }, entryOrThrow),
    update: (id: string, entry: EntryInput) =>
      call(invokeFn, "dictionary_update", { id, entry }, entryOrThrow),
    remove: (id: string) =>
      call(invokeFn, "dictionary_delete", { id }, () => undefined),
    addMany: (
      text: string,
      language: string,
    ): Promise<Outcome<AddManyReport>> =>
      call(invokeFn, "dictionary_add_many", { text, language }, parseReport),
  };
}

export type DictionaryApi = ReturnType<typeof dictionaryApi>;
