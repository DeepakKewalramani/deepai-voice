import React, { useId, useState } from "react";
import "../settings/settings.css";
import "./dictionary.css";
import { BookIcon, LockIcon } from "../ui/icons";
import {
  DictEntry,
  EntryForm,
  FormErrors,
  LANGUAGES,
  Language,
  MAX_ENTRIES,
  checkForm,
  emptyForm,
  formFor,
  languageLabel,
  listView,
  reportText,
} from "./model";
import type { UseDictionary } from "./useDictionary";

const NEXT_DICTATION = "Changes apply from your next dictation.";
const FULL_TEXT = `The dictionary is full (${MAX_ENTRIES} words). Delete some to add more.`;

type Status = { kind: "ok" | "error"; text: string } | null;

const Field: React.FC<{
  label: string;
  hint?: string;
  error?: string;
  children: (props: {
    id: string;
    "aria-invalid": boolean;
    "aria-describedby"?: string;
  }) => React.ReactNode;
  className?: string;
}> = ({ label, hint, error, children, className }) => {
  const id = useId();
  const describedBy =
    [hint && `${id}-hint`, error && `${id}-error`].filter(Boolean).join(" ") ||
    undefined;
  return (
    <div className={`settings-field dict-field ${className ?? ""}`}>
      <label className="settings-label" htmlFor={id}>
        {label}
      </label>
      {children({
        id,
        "aria-invalid": Boolean(error),
        "aria-describedby": describedBy,
      })}
      {hint && !error && (
        <p className="settings-hint" id={`${id}-hint`}>
          {hint}
        </p>
      )}
      {error && (
        <p className="settings-hint dict-error" id={`${id}-error`}>
          {error}
        </p>
      )}
    </div>
  );
};

const LanguageSelect: React.FC<{
  value: Language;
  onChange: (v: Language) => void;
  disabled?: boolean;
  fieldProps: { id: string };
}> = ({ value, onChange, disabled, fieldProps }) => (
  <select
    {...fieldProps}
    className="settings-input dict-input"
    value={value}
    disabled={disabled}
    onChange={(e) => onChange(e.target.value as Language)}
  >
    {LANGUAGES.map((l) => (
      <option key={l.value} value={l.value}>
        {l.label}
      </option>
    ))}
  </select>
);

/** Word, sounds like, language and note, used to add and to edit. */
const EntryFields: React.FC<{
  form: EntryForm;
  errors: FormErrors;
  onChange: (form: EntryForm) => void;
  disabled?: boolean;
  /** Focus the word field on mount (inline edit). */
  autoFocus?: boolean;
}> = ({ form, errors, onChange, disabled, autoFocus }) => {
  const set = (patch: Partial<EntryForm>) => onChange({ ...form, ...patch });
  return (
    <div className="dict-fields">
      <Field label="Word" error={errors.word}>
        {(p) => (
          <input
            {...p}
            className="settings-input dict-input"
            value={form.word}
            maxLength={120}
            autoComplete="off"
            spellCheck={false}
            placeholder="Deepak"
            autoFocus={autoFocus}
            disabled={disabled}
            onChange={(e) => set({ word: e.target.value })}
          />
        )}
      </Field>
      <Field
        label="Sounds like (optional)"
        hint="What the app types instead, comma separated."
        error={errors.soundsLike}
      >
        {(p) => (
          <input
            {...p}
            className="settings-input dict-input"
            value={form.soundsLike}
            autoComplete="off"
            spellCheck={false}
            placeholder="people, deep pack"
            disabled={disabled}
            onChange={(e) => set({ soundsLike: e.target.value })}
          />
        )}
      </Field>
      <Field label="Language" error={errors.language}>
        {(p) => (
          <LanguageSelect
            fieldProps={p}
            value={form.language}
            disabled={disabled}
            onChange={(language) => set({ language })}
          />
        )}
      </Field>
      <Field label="Note (optional)" error={errors.note}>
        {(p) => (
          <input
            {...p}
            className="settings-input dict-input"
            value={form.note}
            maxLength={160}
            autoComplete="off"
            placeholder="My name"
            disabled={disabled}
            onChange={(e) => set({ note: e.target.value })}
          />
        )}
      </Field>
    </div>
  );
};

const AddCard: React.FC<{
  dict: UseDictionary;
  full: boolean;
  onStatus: (s: Status) => void;
}> = ({ dict, full, onStatus }) => {
  const [many, setMany] = useState(false);
  const [form, setForm] = useState<EntryForm>(emptyForm);
  const [errors, setErrors] = useState<FormErrors>({});
  const [text, setText] = useState("");
  const [manyLang, setManyLang] = useState<Language>("any");
  const [busy, setBusy] = useState(false);
  const manyId = useId();

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    const checked = checkForm(form, dict.entries);
    if (!checked.input) {
      setErrors(checked.errors);
      return;
    }
    setBusy(true);
    const r = await dict.add(checked.input);
    setBusy(false);
    if (r.ok) {
      setForm(emptyForm);
      setErrors({});
      onStatus({ kind: "ok", text: `Added. ${NEXT_DICTATION}` });
    } else {
      onStatus({ kind: "error", text: r.error });
    }
  };

  const submitMany = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!text.trim()) return;
    setBusy(true);
    const r = await dict.addMany(text, manyLang);
    setBusy(false);
    if (r.ok) {
      setText("");
      onStatus({
        kind: "ok",
        text: `${reportText(r.value)} ${NEXT_DICTATION}`,
      });
    } else {
      onStatus({ kind: "error", text: r.error });
    }
  };

  return (
    <section
      className="settings-card dict-card"
      aria-labelledby="dict-add-title"
    >
      <div className="dict-card-head">
        <h3 id="dict-add-title" className="section-title dict-title">
          {many ? "Add many words" : "Add a word"}
        </h3>
        <button
          type="button"
          className="dict-link"
          aria-expanded={many}
          onClick={() => setMany((m) => !m)}
        >
          {many ? "Add one word" : "Add many"}
        </button>
      </div>
      {full && (
        <p className="dict-limit" role="note">
          {FULL_TEXT}
        </p>
      )}
      {many ? (
        <form className="dict-form" onSubmit={submitMany}>
          <div className="settings-field dict-field">
            <label className="settings-label" htmlFor={manyId}>
              Words, one per line
            </label>
            <textarea
              id={manyId}
              className="settings-input dict-input dict-textarea"
              rows={5}
              value={text}
              spellCheck={false}
              placeholder={"Deepak\nTequity\nKubernetes"}
              disabled={busy || full}
              onChange={(e) => setText(e.target.value)}
            />
            <p className="settings-hint">
              Blank lines and words already in the dictionary are skipped.
            </p>
          </div>
          <div className="dict-form-foot">
            <Field label="Language" className="dict-field-inline">
              {(p) => (
                <LanguageSelect
                  fieldProps={p}
                  value={manyLang}
                  disabled={busy || full}
                  onChange={setManyLang}
                />
              )}
            </Field>
            <button
              type="submit"
              className="settings-btn settings-btn-primary"
              disabled={busy || full || !text.trim()}
            >
              Add all
            </button>
          </div>
        </form>
      ) : (
        <form className="dict-form" onSubmit={submit} noValidate>
          <EntryFields
            form={form}
            errors={errors}
            disabled={busy || full}
            onChange={(f) => {
              setForm(f);
              if (Object.keys(errors).length) setErrors({});
            }}
          />
          <div className="dict-form-foot dict-form-foot-end">
            <button
              type="submit"
              className="settings-btn settings-btn-primary"
              disabled={busy || full}
            >
              Add word
            </button>
          </div>
        </form>
      )}
    </section>
  );
};

const EntryRow: React.FC<{
  entry: DictEntry;
  dict: UseDictionary;
  onStatus: (s: Status) => void;
}> = ({ entry, dict, onStatus }) => {
  const [mode, setMode] = useState<"view" | "edit" | "confirm">("view");
  const [form, setForm] = useState<EntryForm>(() => formFor(entry));
  const [errors, setErrors] = useState<FormErrors>({});
  const [busy, setBusy] = useState(false);

  const save = async (e: React.FormEvent) => {
    e.preventDefault();
    const checked = checkForm(form, dict.entries, entry.id);
    if (!checked.input) {
      setErrors(checked.errors);
      return;
    }
    setBusy(true);
    const r = await dict.update(entry.id, checked.input);
    setBusy(false);
    if (r.ok) {
      setMode("view");
      onStatus({ kind: "ok", text: `Saved. ${NEXT_DICTATION}` });
    } else {
      onStatus({ kind: "error", text: r.error });
    }
  };

  const remove = async () => {
    setBusy(true);
    const r = await dict.remove(entry.id);
    setBusy(false);
    if (r.ok) onStatus({ kind: "ok", text: `Deleted. ${NEXT_DICTATION}` });
    else {
      setMode("view");
      onStatus({ kind: "error", text: r.error });
    }
  };

  const lang = entry.language === "any" ? undefined : entry.language;

  if (mode === "edit") {
    return (
      <li className="dict-row dict-row-edit">
        <form
          className="dict-form"
          onSubmit={save}
          noValidate
          aria-label={`Edit ${entry.word}`}
          onKeyDown={(e) => {
            if (e.key === "Escape") setMode("view");
          }}
        >
          <EntryFields
            form={form}
            errors={errors}
            disabled={busy}
            autoFocus
            onChange={(f) => {
              setForm(f);
              if (Object.keys(errors).length) setErrors({});
            }}
          />
          <div className="dict-form-foot dict-form-foot-end">
            <button
              type="button"
              className="settings-btn"
              onClick={() => setMode("view")}
            >
              Cancel
            </button>
            <button
              type="submit"
              className="settings-btn settings-btn-primary"
              disabled={busy}
            >
              Save
            </button>
          </div>
        </form>
      </li>
    );
  }

  return (
    <li className="dict-row">
      <div className="dict-row-text">
        <div className="dict-row-title">
          <strong className="dict-word" lang={lang}>
            {entry.word}
          </strong>
          <span className="dict-lang">{languageLabel(entry.language)}</span>
        </div>
        {entry.hearedAs.length > 0 && (
          <p className="dict-meta">
            Sounds like <span lang={lang}>{entry.hearedAs.join(", ")}</span>
          </p>
        )}
        {entry.note && <p className="dict-meta dict-note">{entry.note}</p>}
      </div>
      {mode === "confirm" ? (
        <div
          className="dict-actions dict-confirm"
          role="group"
          aria-label={`Delete ${entry.word}?`}
        >
          <span className="dict-confirm-text">Delete?</span>
          <button
            type="button"
            className="settings-btn dict-small"
            autoFocus
            onClick={() => setMode("view")}
          >
            Cancel
          </button>
          <button
            type="button"
            className="settings-btn dict-small dict-danger"
            disabled={busy}
            onClick={remove}
          >
            Delete
          </button>
        </div>
      ) : (
        <div className="dict-actions">
          <button
            type="button"
            className="settings-btn dict-small"
            aria-label={`Edit ${entry.word}`}
            onClick={() => {
              setForm(formFor(entry));
              setErrors({});
              setMode("edit");
            }}
          >
            Edit
          </button>
          <button
            type="button"
            className="settings-btn dict-small"
            aria-label={`Delete ${entry.word}`}
            onClick={() => setMode("confirm")}
          >
            Delete
          </button>
        </div>
      )}
    </li>
  );
};

/** The Dictionary page body, driven by `useDictionary` (or a test double). */
export const DictionaryPage: React.FC<{ dict: UseDictionary }> = ({ dict }) => {
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState<Status>(null);
  const view = listView(dict.entries, query);
  const ready = dict.state === "ready";
  const searchId = useId();

  return (
    <>
      <section className="note-card" aria-labelledby="dict-privacy-title">
        <span className="note-icon">
          <LockIcon />
        </span>
        <div className="note-body">
          <h3 id="dict-privacy-title" className="note-title">
            Stored only on this Mac
          </h3>
          <p className="note-text">
            Add names and terms the app often gets wrong. Words are stored only
            on this Mac and are never uploaded. They help, but cannot guarantee
            a perfect result.
          </p>
        </div>
      </section>

      {ready && <AddCard dict={dict} full={view.full} onStatus={setStatus} />}

      <p
        className="dict-status"
        role="status"
        aria-live="polite"
        data-kind={status?.kind}
      >
        {status?.text ?? ""}
      </p>

      <section className="dict-list-section" aria-labelledby="dict-list-title">
        <div className="dict-list-head">
          <div className="dict-list-titlewrap">
            <h3 id="dict-list-title" className="section-title dict-title">
              Your words
            </h3>
            {ready && <span className="dict-count">{view.countLabel}</span>}
          </div>
          {ready && !view.empty && (
            <>
              <label className="dict-sr" htmlFor={searchId}>
                Search words
              </label>
              <input
                id={searchId}
                type="search"
                className="settings-input dict-input dict-search"
                placeholder="Search"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </>
          )}
        </div>
        <p className="settings-hint dict-next">
          Used for your next dictation. Changes do not affect a dictation that
          is already running.
        </p>

        {dict.state === "loading" && (
          <p className="dict-message" role="status">
            Loading your dictionary...
          </p>
        )}

        {dict.state === "error" && (
          <div className="dict-error-card" role="alert">
            <div>
              <strong>Could not load the dictionary</strong>
              <p className="settings-hint">{dict.error}</p>
            </div>
            <button
              type="button"
              className="settings-btn"
              onClick={dict.reload}
            >
              Try again
            </button>
          </div>
        )}

        {ready && view.empty && (
          <div className="empty-card dict-empty">
            <span className="empty-icon">
              <BookIcon />
            </span>
            <h4 className="empty-title">No words yet</h4>
            <p className="empty-text">
              Add a name or term above, for example your own name, so it is
              spelled your way.
            </p>
          </div>
        )}

        {ready && !view.empty && view.shown.length === 0 && (
          <p className="dict-message">No words match your search.</p>
        )}

        {ready && view.shown.length > 0 && (
          <ul className="settings-card dict-list" aria-label="Dictionary words">
            {view.shown.map((e) => (
              <EntryRow key={e.id} entry={e} dict={dict} onStatus={setStatus} />
            ))}
          </ul>
        )}
      </section>
    </>
  );
};
