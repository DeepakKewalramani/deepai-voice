/**
 * In-memory fake of the Rust dictionary commands. Used by tests and by the
 * dev browser preview only (`npm run dev`, outside Tauri): `?dictionary=empty`,
 * `?dictionary=error`, `?dictionary=full`, otherwise sample entries. Imported
 * behind `import.meta.env.DEV`, so production builds drop it. Never stores
 * anything on disk.
 */
import type { Invoke } from "./api";
import {
  DictEntry,
  EntryInput,
  MAX_ENTRIES,
  isDuplicate,
  validateHeardAs,
  validateLanguage,
  validateNote,
  validateWord,
} from "./model";

export const SAMPLE_ENTRIES: DictEntry[] = [
  {
    id: "s1",
    word: "Deepak",
    hearedAs: ["people", "deep pack"],
    language: "any",
    note: "My name",
  },
  { id: "s2", word: "DeepakKewalramani", hearedAs: [], language: "any", note: null },
  {
    id: "s3",
    word: "Kubernetes",
    hearedAs: ["cuban eighties"],
    language: "en",
    note: null,
  },
  { id: "s4", word: "नमस्ते", hearedAs: [], language: "hi", note: null },
];

function check(input: EntryInput, id: string): DictEntry {
  const word = validateWord(input.word);
  if (!word.ok) throw word.error;
  const heard = validateHeardAs(input.hearedAs ?? []);
  if (!heard.ok) throw heard.error;
  const lang = validateLanguage(input.language);
  if (!lang.ok) throw lang.error;
  const note = validateNote(input.note ?? "");
  if (!note.ok) throw note.error;
  return {
    id,
    word: word.value,
    hearedAs: heard.value,
    language: lang.value,
    note: note.value,
  };
}

/** A fake `invoke` over an in-memory list, with the Rust rules and errors. */
export function memoryInvoke(
  initial: DictEntry[] = SAMPLE_ENTRIES,
  options: { failList?: boolean } = {},
): Invoke {
  let entries = initial.map((e) => ({ ...e, hearedAs: [...e.hearedAs] }));
  let next = 1;
  const newId = () => `m${next++}`;
  return async (cmd, args = {}) => {
    switch (cmd) {
      case "dictionary_list":
        if (options.failList) throw "Could not read the dictionary.";
        return entries.map((e) => ({ ...e }));
      case "dictionary_add": {
        const entry = check(args.entry as EntryInput, newId());
        if (isDuplicate(entries, entry.word))
          throw "This word is already in the dictionary.";
        if (entries.length >= MAX_ENTRIES)
          throw "The dictionary is full (500 words). Delete some to add more.";
        entries = [...entries, entry];
        return entry;
      }
      case "dictionary_update": {
        const id = String(args.id);
        if (!entries.some((e) => e.id === id))
          throw "That entry no longer exists.";
        const entry = check(args.entry as EntryInput, id);
        if (isDuplicate(entries, entry.word, id))
          throw "This word is already in the dictionary.";
        entries = entries.map((e) => (e.id === id ? entry : e));
        return entry;
      }
      case "dictionary_delete": {
        const id = String(args.id);
        if (!entries.some((e) => e.id === id))
          throw "That entry no longer exists.";
        entries = entries.filter((e) => e.id !== id);
        return null;
      }
      case "dictionary_add_many": {
        const lang = validateLanguage(String(args.language));
        if (!lang.ok) throw lang.error;
        const report = { added: 0, duplicates: 0, invalid: 0, overLimit: 0 };
        for (const line of String(args.text).split(/\r?\n/)) {
          if (!line.trim()) continue;
          const word = validateWord(line);
          if (!word.ok) report.invalid++;
          else if (isDuplicate(entries, word.value)) report.duplicates++;
          else if (entries.length >= MAX_ENTRIES) report.overLimit++;
          else {
            entries = [
              ...entries,
              {
                id: newId(),
                word: word.value,
                hearedAs: [],
                language: lang.value,
                note: null,
              },
            ];
            report.added++;
          }
        }
        return report;
      }
      default:
        throw "Unknown command.";
    }
  };
}

/** Fake backend for the dev browser preview, from the page's query string. */
export function devPreviewInvoke(search: string): Invoke {
  const mode = new URLSearchParams(search).get("dictionary");
  if (mode === "empty") return memoryInvoke([]);
  if (mode === "error") return memoryInvoke([], { failList: true });
  if (mode === "full")
    return memoryInvoke(
      Array.from({ length: MAX_ENTRIES }, (_, i) => ({
        id: `f${i}`,
        word: `Term ${i + 1}`,
        hearedAs: [],
        language: "any",
        note: null,
      })),
    );
  return memoryInvoke();
}
