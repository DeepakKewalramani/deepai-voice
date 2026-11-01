import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import vectors from "../validation-vectors.json";
import {
  DictEntry,
  MAX_ENTRIES,
  MAX_HEARD_AS,
  MAX_HEARD_AS_CHARS,
  MAX_NOTE_CHARS,
  MAX_WORD_CHARS,
  checkForm,
  emptyForm,
  formFor,
  listView,
  parseEntries,
  parseSoundsLike,
  reportText,
  validateHeardAs,
  validateLanguage,
  validateNote,
  validateWord,
} from "../model";
import { dictionaryApi } from "../api";
import { SAMPLE_ENTRIES, memoryInvoke } from "../preview";
import type { UseDictionary } from "../useDictionary";
import { DictionaryView } from "../../ui/views";

const entry = (word: string, over: Partial<DictEntry> = {}): DictEntry => ({
  id: `id-${word}`,
  word,
  hearedAs: [],
  language: "any",
  note: null,
  ...over,
});

describe("shared validation vectors (same file as the Rust test)", () => {
  it("limits match", () => {
    expect(vectors.limits).toEqual({
      maxEntries: MAX_ENTRIES,
      maxWordChars: MAX_WORD_CHARS,
      maxHeardAs: MAX_HEARD_AS,
      maxHeardAsChars: MAX_HEARD_AS_CHARS,
      maxNoteChars: MAX_NOTE_CHARS,
    });
  });

  it("word", () => {
    for (const [raw, want] of vectors.word.valid)
      expect(validateWord(raw)).toEqual({ ok: true, value: want });
    for (const raw of vectors.word.invalid)
      expect(validateWord(raw).ok, JSON.stringify(raw)).toBe(false);
  });

  it("sounds like list", () => {
    for (const [raw, want] of vectors.heardAs.valid)
      expect(validateHeardAs(raw)).toEqual({ ok: true, value: want });
    for (const raw of vectors.heardAs.invalid)
      expect(validateHeardAs(raw).ok, JSON.stringify(raw)).toBe(false);
  });

  it("language and note", () => {
    for (const raw of vectors.language.valid)
      expect(validateLanguage(raw).ok).toBe(true);
    for (const raw of vectors.language.invalid)
      expect(validateLanguage(raw).ok, raw).toBe(false);
    for (const [raw, want] of vectors.note.valid)
      expect(validateNote(raw as string)).toEqual({ ok: true, value: want });
    for (const raw of vectors.note.invalid)
      expect(validateNote(raw).ok).toBe(false);
  });
});

describe("parseSoundsLike", () => {
  it("splits on commas, trims and drops empty items", () => {
    expect(parseSoundsLike("people, peoples ,, deep pack ")).toEqual([
      "people",
      "peoples",
      "deep pack",
    ]);
    expect(parseSoundsLike("")).toEqual([]);
    expect(parseSoundsLike(" , ")).toEqual([]);
    expect(parseSoundsLike("दिपक, zoe")).toEqual(["दिपक", "zoe"]);
  });
});

describe("checkForm", () => {
  it("builds the input and rejects case-insensitive duplicates", () => {
    const list = [entry("Deepak")];
    const ok = checkForm(
      { word: " DeepakKewalramani ", soundsLike: "tech city", language: "en", note: "" },
      list,
    );
    expect(ok.input).toEqual({
      word: "DeepakKewalramani",
      hearedAs: ["tech city"],
      language: "en",
      note: null,
    });
    const dup = checkForm({ ...emptyForm, word: "DEEPAK" }, list);
    expect(dup.errors?.word).toBe("This word is already in the dictionary.");
    // Editing an entry may keep its own word.
    expect(
      checkForm({ ...emptyForm, word: "deepak" }, list, "id-Deepak").input,
    ).not.toBeNull();
    const many = checkForm(
      { ...emptyForm, word: "x", soundsLike: "a,b,c,d,e,f" },
      list,
    );
    expect(many.errors?.soundsLike).toBe(
      "Use at most 5 sound-alike spellings.",
    );
    expect(checkForm(emptyForm, list).errors?.word).toBe("Enter a word.");
  });

  it("formFor round-trips an entry", () => {
    const e = entry("Deepak", {
      hearedAs: ["people", "deep pack"],
      note: "Me",
    });
    expect(formFor(e)).toEqual({
      word: "Deepak",
      soundsLike: "people, deep pack",
      language: "any",
      note: "Me",
    });
  });
});

describe("list view model", () => {
  const list = [
    entry("Deepak", { hearedAs: ["people"] }),
    entry("DeepakKewalramani", { note: "company" }),
    entry("Kotlin"),
  ];

  it("counts, searches word, sounds like and note", () => {
    expect(listView(list, "").countLabel).toBe("3 words");
    expect(listView(list, "PEOPLE").shown.map((e) => e.word)).toEqual([
      "Deepak",
    ]);
    expect(listView(list, "comp").countLabel).toBe("1 of 3 words");
    expect(listView([], "").empty).toBe(true);
    expect(listView([entry("a")], "").countLabel).toBe("1 word");
  });

  it("is full at 500", () => {
    const full = Array.from({ length: MAX_ENTRIES }, (_, i) => entry(`w${i}`));
    expect(listView(full, "").full).toBe(true);
    expect(listView(full.slice(1), "").full).toBe(false);
  });

  it("reports add many counts in one sentence", () => {
    expect(
      reportText({ added: 3, duplicates: 2, invalid: 1, overLimit: 0 }),
    ).toBe("Added 3 words, 2 already there, 1 not valid.");
    expect(
      reportText({ added: 1, duplicates: 0, invalid: 0, overLimit: 4 }),
    ).toBe("Added 1 word, 4 over the 500 limit.");
  });

  it("parses only well formed entries", () => {
    expect(
      parseEntries([
        { id: "1", word: "A", hearedAs: ["b", 3], language: "en" },
        { id: "", word: "x" },
        null,
      ]),
    ).toEqual([
      { id: "1", word: "A", hearedAs: ["b"], language: "en", note: null },
    ]);
    expect(parseEntries("nope")).toEqual([]);
  });
});

describe("dictionaryApi over the in-memory store", () => {
  it("adds, updates, deletes and adds many with the Rust rules", async () => {
    const api = dictionaryApi(memoryInvoke([entry("Deepak")]));
    const added = await api.add({
      word: "DeepakKewalramani",
      hearedAs: ["tech city"],
      language: "any",
      note: null,
    });
    expect(added.ok).toBe(true);
    const dup = await api.add({
      word: "deepak",
      hearedAs: [],
      language: "any",
      note: null,
    });
    expect(dup).toEqual({
      ok: false,
      error: "This word is already in the dictionary.",
    });
    const many = await api.addMany(
      "Rust\n\nrust\nDeepak\n" + "z".repeat(61),
      "en",
    );
    expect(many).toEqual({
      ok: true,
      value: { added: 1, duplicates: 2, invalid: 1, overLimit: 0 },
    });
    const list = await api.list();
    expect(list.ok && list.value.map((e) => e.word)).toEqual([
      "Deepak",
      "DeepakKewalramani",
      "Rust",
    ]);
    expect((await api.remove("id-Deepak")).ok).toBe(true);
    expect(await api.remove("id-Deepak")).toEqual({
      ok: false,
      error: "That entry no longer exists.",
    });
  });

  it("sends the command names and arguments the Rust side expects", async () => {
    const invoke = vi.fn(async () => ({ id: "1", word: "Deepak" }));
    const api = dictionaryApi(invoke);
    const input = {
      word: "Deepak",
      hearedAs: ["people"],
      language: "any",
      note: null,
    };
    await api.add(input);
    await api.update("1", input);
    expect(invoke).toHaveBeenNthCalledWith(1, "dictionary_add", {
      entry: input,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "dictionary_update", {
      id: "1",
      entry: input,
    });
  });

  it("never throws: rejections and missing backend become messages", async () => {
    const failing = dictionaryApi(async () => {
      throw new Error("boom");
    });
    expect(await failing.list()).toEqual({
      ok: false,
      error: "Something went wrong. Try again.",
    });
    expect(await dictionaryApi(undefined).list()).toEqual({
      ok: false,
      error: "Available in the app only.",
    });
  });
});

const fakeDict = (over: Partial<UseDictionary> = {}): UseDictionary => ({
  entries: [],
  state: "ready",
  error: "",
  reload: () => {},
  add: async () => ({ ok: false, error: "x" }),
  update: async () => ({ ok: false, error: "x" }),
  remove: async () => ({ ok: true, value: undefined }),
  addMany: async () => ({ ok: false, error: "x" }),
  ...over,
});

const render = (dict: UseDictionary) =>
  renderToStaticMarkup(<DictionaryView dict={dict} />);

describe("DictionaryView", () => {
  it("shows the intro, add form and next-dictation note", () => {
    const m = render(fakeDict({ entries: SAMPLE_ENTRIES }));
    expect(m).toContain(
      "Add names and terms the app often gets wrong. Words are stored only on this Mac and are never uploaded. They help, but cannot guarantee a perfect result.",
    );
    expect(m).toContain("Used for your next dictation.");
    for (const label of [
      "Word",
      "Sounds like (optional)",
      "Language",
      "Note (optional)",
    ])
      expect(m).toContain(`>${label}</label>`);
    expect(m).toContain('<option value="hi">Hindi</option>');
    expect(m).toContain('aria-expanded="false"');
    expect(m).not.toContain("Coming soon");
  });

  it("lists entries with count, search and labelled actions", () => {
    const m = render(fakeDict({ entries: SAMPLE_ENTRIES }));
    expect(m).toContain("4 words");
    expect(m).toContain(">Search words</label>");
    expect(m).toContain('aria-label="Edit Deepak"');
    expect(m).toContain('aria-label="Delete Deepak"');
    expect(m).toContain("people, deep pack");
    expect(m).toContain('lang="hi"');
  });

  it("shows the empty state", () => {
    const m = render(fakeDict());
    expect(m).toContain("No words yet");
    expect(m).toContain("0 words");
    expect(m).not.toContain("Search words");
  });

  it("shows loading and error states without the form", () => {
    const loading = render(fakeDict({ state: "loading" }));
    expect(loading).toContain("Loading your dictionary...");
    expect(loading).not.toContain("Add word");
    const error = render(
      fakeDict({ state: "error", error: "Could not read the dictionary." }),
    );
    expect(error).toContain('role="alert"');
    expect(error).toContain("Could not read the dictionary.");
    expect(error).toContain("Try again");
  });

  it("shows the limit message and disables adding at 500", () => {
    const full = Array.from({ length: MAX_ENTRIES }, (_, i) => entry(`w${i}`));
    const m = render(fakeDict({ entries: full }));
    expect(m).toContain(
      "The dictionary is full (500 words). Delete some to add more.",
    );
    expect(m).toMatch(/<button type="submit"[^>]*disabled=""[^>]*>Add word/);
  });
});
