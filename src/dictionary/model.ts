/**
 * Dictionary page logic, all pure: types, parsing of the Rust payloads,
 * validation mirrored from `src-tauri/src/dictionary/mod.rs` (the shared
 * cases live in `validation-vectors.json`) and the list view model.
 */

export const MAX_ENTRIES = 500;
export const MAX_WORD_CHARS = 60;
export const MAX_HEARD_AS = 5;
export const MAX_HEARD_AS_CHARS = 60;
export const MAX_NOTE_CHARS = 80;

export type Language = "any" | "en" | "hi";

export const LANGUAGES: { value: Language; label: string }[] = [
  { value: "any", label: "Any" },
  { value: "en", label: "English" },
  { value: "hi", label: "Hindi" },
];

/** One entry, as sent by `dictionary_list`. */
export interface DictEntry {
  id: string;
  word: string;
  hearedAs: string[];
  language: string;
  note: string | null;
}

/** Fields sent to `dictionary_add` and `dictionary_update`. */
export interface EntryInput {
  word: string;
  hearedAs: string[];
  language: string;
  note: string | null;
}

/** Counts returned by `dictionary_add_many`. */
export interface AddManyReport {
  added: number;
  duplicates: number;
  invalid: number;
  overLimit: number;
}

/** Form state for add and inline edit. "Sounds like" is a comma list. */
export interface EntryForm {
  word: string;
  soundsLike: string;
  language: Language;
  note: string;
}

export const emptyForm: EntryForm = {
  word: "",
  soundsLike: "",
  language: "any",
  note: "",
};

const str = (v: unknown) => (typeof v === "string" ? v : "");

export function parseEntry(raw: unknown): DictEntry | null {
  if (typeof raw !== "object" || raw === null) return null;
  const r = raw as Record<string, unknown>;
  const id = str(r.id);
  const word = str(r.word);
  if (!id || !word) return null;
  return {
    id,
    word,
    hearedAs: Array.isArray(r.hearedAs)
      ? r.hearedAs.filter((h): h is string => typeof h === "string")
      : [],
    language: str(r.language) || "any",
    note: typeof r.note === "string" && r.note ? r.note : null,
  };
}

export function parseEntries(raw: unknown): DictEntry[] {
  if (!Array.isArray(raw)) return [];
  return raw.map(parseEntry).filter((e): e is DictEntry => e !== null);
}

export function parseReport(raw: unknown): AddManyReport {
  const r = (raw ?? {}) as Record<string, unknown>;
  const n = (v: unknown) => (typeof v === "number" && v >= 0 ? v : 0);
  return {
    added: n(r.added),
    duplicates: n(r.duplicates),
    invalid: n(r.invalid),
    overLimit: n(r.overLimit),
  };
}

// ---------- validation (mirrors Rust) ----------

// Unicode Cc, the same set as Rust's `char::is_control`.
// eslint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u001f\u007f-\u009f]/;
const chars = (s: string) => Array.from(s).length;

export type Checked<T> = { ok: true; value: T } | { ok: false; error: string };

const fail = (error: string): { ok: false; error: string } => ({
  ok: false,
  error,
});

export function validateWord(raw: string): Checked<string> {
  const word = raw.trim();
  if (!word) return fail("Enter a word.");
  if (CONTROL.test(word))
    return fail("The word cannot contain tabs or line breaks.");
  if (chars(word) > MAX_WORD_CHARS)
    return fail("Use at most 60 characters for the word.");
  return { ok: true, value: word };
}

export function validateHeardAs(raw: string[]): Checked<string[]> {
  const out: string[] = [];
  for (const item of raw.map((s) => s.trim())) {
    if (!item) continue;
    if (CONTROL.test(item))
      return fail("Sound-alike spellings cannot contain tabs or line breaks.");
    if (chars(item) > MAX_HEARD_AS_CHARS)
      return fail("Each sound-alike spelling can have at most 60 characters.");
    if (!out.some((o) => o.toLowerCase() === item.toLowerCase()))
      out.push(item);
  }
  if (out.length > MAX_HEARD_AS)
    return fail("Use at most 5 sound-alike spellings.");
  return { ok: true, value: out };
}

export function validateLanguage(raw: string): Checked<string> {
  return raw === "any" || /^[a-z]{2,3}$/.test(raw)
    ? { ok: true, value: raw }
    : fail("Choose a language.");
}

export function validateNote(raw: string): Checked<string | null> {
  const note = raw.trim();
  if (!note) return { ok: true, value: null };
  if (CONTROL.test(note))
    return fail("The note cannot contain tabs or line breaks.");
  if (chars(note) > MAX_NOTE_CHARS)
    return fail("Use at most 80 characters for the note.");
  return { ok: true, value: note };
}

/** "people, peoples ,, deep pack" -> ["people", "peoples", "deep pack"]. */
export function parseSoundsLike(text: string): string[] {
  return text
    .split(/[,\n]/)
    .map((s) => s.trim())
    .filter(Boolean);
}

export function formatSoundsLike(list: string[]): string {
  return list.join(", ");
}

export type FormErrors = Partial<Record<keyof EntryForm, string>>;

/** Checks a form; returns the input to send, or the errors per field. */
export function checkForm(
  form: EntryForm,
  entries: DictEntry[],
  editingId: string | null = null,
): { input: EntryInput; errors: null } | { input: null; errors: FormErrors } {
  const errors: FormErrors = {};
  const word = validateWord(form.word);
  const heard = validateHeardAs(parseSoundsLike(form.soundsLike));
  const lang = validateLanguage(form.language);
  const note = validateNote(form.note);
  if (!word.ok) errors.word = word.error;
  else if (isDuplicate(entries, word.value, editingId))
    errors.word = "This word is already in the dictionary.";
  if (!heard.ok) errors.soundsLike = heard.error;
  if (!lang.ok) errors.language = lang.error;
  if (!note.ok) errors.note = note.error;
  if (!word.ok || !heard.ok || !lang.ok || !note.ok || errors.word)
    return { input: null, errors };
  return {
    input: {
      word: word.value,
      hearedAs: heard.value,
      language: lang.value,
      note: note.value,
    },
    errors: null,
  };
}

export function isDuplicate(
  entries: DictEntry[],
  word: string,
  exceptId: string | null = null,
): boolean {
  const key = word.trim().toLowerCase();
  return entries.some(
    (e) => e.id !== exceptId && e.word.trim().toLowerCase() === key,
  );
}

export function formFor(entry: DictEntry): EntryForm {
  const lang = LANGUAGES.find((l) => l.value === entry.language);
  return {
    word: entry.word,
    soundsLike: formatSoundsLike(entry.hearedAs),
    language: lang ? lang.value : "any",
    note: entry.note ?? "",
  };
}

// ---------- view model ----------

export function languageLabel(code: string): string {
  return LANGUAGES.find((l) => l.value === code)?.label ?? code.toUpperCase();
}

/** Entries whose word, sound-alike spelling or note contains `query`. */
export function filterEntries(
  entries: DictEntry[],
  query: string,
): DictEntry[] {
  const q = query.trim().toLowerCase();
  if (!q) return entries;
  return entries.filter(
    (e) =>
      e.word.toLowerCase().includes(q) ||
      e.hearedAs.some((h) => h.toLowerCase().includes(q)) ||
      (e.note ?? "").toLowerCase().includes(q),
  );
}

export interface ListView {
  shown: DictEntry[];
  /** "3 words", "1 of 3 words". */
  countLabel: string;
  full: boolean;
  /** No entries at all (not just none matching the search). */
  empty: boolean;
}

const words = (n: number) => `${n} ${n === 1 ? "word" : "words"}`;

export function listView(entries: DictEntry[], query: string): ListView {
  const shown = filterEntries(entries, query);
  const total = entries.length;
  return {
    shown,
    countLabel:
      shown.length === total
        ? words(total)
        : `${shown.length} of ${words(total)}`,
    full: total >= MAX_ENTRIES,
    empty: total === 0,
  };
}

/** One sentence for the "Add many" result. */
export function reportText(r: AddManyReport): string {
  const parts = [`Added ${words(r.added)}`];
  if (r.duplicates) parts.push(`${r.duplicates} already there`);
  if (r.invalid) parts.push(`${r.invalid} not valid`);
  if (r.overLimit) parts.push(`${r.overLimit} over the 500 limit`);
  return `${parts.join(", ")}.`;
}
