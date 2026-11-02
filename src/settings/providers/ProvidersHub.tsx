import React, { useState } from "react";
import "./providers.css";
import type { Provider } from "../types";
import { LockIcon, LogoMark } from "../../ui/icons";
import {
  CAPABILITY_LABEL,
  CardModel,
  Chip,
  buildSections,
  customProviders,
  hindiLabel,
  maskTail,
  streamingLabel,
  testLine,
} from "./model";
import type { ProviderStatus, TestResult } from "./types";
import {
  CustomForm,
  FormErrors,
  authFromSetting,
  emptyForm,
  isLocalUrl,
  validateForm,
} from "./validate";
import { useProviders } from "./useProviders";

export const INTRO =
  "Connected providers are saved on this Mac. Using them for dictation arrives with the cloud adapters, so today dictation still uses the built-in engines.";

const StatusPill: React.FC<{ chip: Chip }> = ({ chip }) => (
  <span className="perm-pill hub-pill" data-tone={chip.tone}>
    <span className="perm-dot" aria-hidden="true" />
    {chip.label}
  </span>
);

// ---------- catalog card ----------

export interface CardProps {
  card: CardModel;
  connecting: boolean;
  keyDraft: string;
  busy: boolean;
  onConnect: (id: string) => void;
  onKeyChange: (value: string) => void;
  onSaveKey: (id: string) => void;
  onCancel: () => void;
  onDisconnect: (id: string) => void;
  onDocs: (id: string) => void;
  onSetup: (card: CardModel) => void;
}

export const ProviderCard: React.FC<CardProps> = ({
  card,
  connecting,
  keyDraft,
  busy,
  onConnect,
  onKeyChange,
  onSaveKey,
  onCancel,
  onDisconnect,
  onDocs,
  onSetup,
}) => {
  const { entry: e, action } = card;
  const nameId = `hub-${e.id}-name`;
  const keyId = `hub-key-${e.id}`;
  const docs = e.docsUrl ? (
    <button
      type="button"
      className="hub-link"
      onClick={() => onDocs(e.id)}
      aria-label={`${action === "connect" || action === "manage" ? "Get a key" : "Docs"} for ${e.name} (opens the browser)`}
    >
      {action === "connect" || action === "manage" ? "Get a key" : "Docs"}
    </button>
  ) : null;
  const status =
    card.status.tone === "ok"
      ? "connected"
      : card.status.tone === "builtin"
        ? "builtin"
        : card.status.tone === "warn"
          ? "candidate"
          : "idle";
  return (
    <article
      className="settings-card hub-card"
      data-status={status}
      aria-labelledby={nameId}
    >
      <div className="hub-card-head">
        <div className="hub-card-titlewrap">
          <span className="hub-brand" aria-hidden="true">
            {e.status === "builtin" ? <LogoMark /> : e.name.charAt(0)}
          </span>
          <div className="hub-card-title">
            <strong id={nameId}>{e.name}</strong>
            <span className="settings-hint">{e.vendor}</span>
          </div>
        </div>
        <StatusPill chip={card.status} />
      </div>
      <ul className="hub-chips" aria-label="Details">
        <li className="hub-chip" data-on={e.hindi === "yes"}>
          {e.hindi === "yes" && (
            <span className="hub-chip-led" aria-hidden="true" />
          )}
          {hindiLabel(e.hindi)}
        </li>
        <li
          className="hub-chip"
          data-kind="streaming"
          data-on={e.streaming === "yes"}
        >
          {e.streaming === "yes" && (
            <span className="hub-chip-led" aria-hidden="true" />
          )}
          {streamingLabel(e.streaming)}
        </li>
        <li className="hub-chip hub-chip-price">{e.price}</li>
      </ul>
      <p className="settings-hint hub-note">
        {e.languages}. {e.note}
      </p>
      {card.keyTail !== null && action === "manage" && (
        <p className="settings-hint hub-keyline">
          <LockIcon />
          Key <span className="hub-mask">{maskTail(card.keyTail)}</span>
        </p>
      )}
      {e.status === "candidate" && (
        <p className="settings-hint hub-candidate">
          Not verified yet, so it cannot be connected.
        </p>
      )}
      {connecting ? (
        <form
          className="settings-key hub-keyfield"
          onSubmit={(ev) => {
            ev.preventDefault();
            onSaveKey(e.id);
          }}
          onKeyDown={(ev) => {
            if (ev.key === "Escape") onCancel();
          }}
        >
          <label className="settings-label" htmlFor={keyId}>
            API key for {e.name}
          </label>
          <input
            id={keyId}
            className="settings-input"
            type="password"
            autoComplete="off"
            spellCheck={false}
            autoFocus
            value={keyDraft}
            aria-describedby={`${keyId}-hint`}
            onChange={(ev) => onKeyChange(ev.target.value)}
          />
          <p className="settings-hint" id={`${keyId}-hint`}>
            Saved in the macOS Keychain. Only the last 4 characters are shown.
          </p>
          <div className="hub-actions">
            <button
              type="submit"
              className="settings-btn settings-btn-primary hub-btn"
              disabled={busy || keyDraft.trim().length === 0}
            >
              Save
            </button>
            <button
              type="button"
              className="settings-btn hub-btn"
              onClick={onCancel}
            >
              Cancel
            </button>
            {docs}
          </div>
        </form>
      ) : (
        (action !== "none" || docs) && (
          <div className="hub-actions">
            {action === "connect" && (
              <button
                type="button"
                className="settings-btn settings-btn-primary hub-btn"
                onClick={() => onConnect(e.id)}
              >
                Connect
              </button>
            )}
            {action === "manage" && (
              <>
                <button
                  type="button"
                  className="settings-btn hub-btn"
                  onClick={() => onConnect(e.id)}
                >
                  Replace key
                </button>
                <button
                  type="button"
                  className="settings-btn hub-btn"
                  disabled={busy}
                  onClick={() => onDisconnect(e.id)}
                >
                  Disconnect
                </button>
              </>
            )}
            {action === "setup" && (
              <button
                type="button"
                className="settings-btn hub-btn"
                onClick={() => onSetup(card)}
              >
                Set up
              </button>
            )}
            {docs}
          </div>
        )
      )}
    </article>
  );
};

// ---------- make your own ----------

export interface CustomFormProps {
  form: CustomForm;
  errors: FormErrors;
  keyDraft: string;
  busy: boolean;
  onChange: (form: CustomForm) => void;
  onKeyChange: (value: string) => void;
  onSubmit: () => void;
  onCancel: () => void;
}

const Field: React.FC<{
  id: string;
  label: string;
  error?: string;
  hint?: string;
  children: (describedBy: string | undefined) => React.ReactNode;
}> = ({ id, label, error, hint, children }) => {
  const ids = [error && `${id}-error`, hint && `${id}-hint`]
    .filter(Boolean)
    .join(" ");
  return (
    <div className="settings-field hub-field">
      <label className="settings-label" htmlFor={id}>
        {label}
      </label>
      {children(ids || undefined)}
      {error && (
        <p className="hub-error" id={`${id}-error`}>
          {error}
        </p>
      )}
      {hint && (
        <p className="settings-hint" id={`${id}-hint`}>
          {hint}
        </p>
      )}
    </div>
  );
};

export const CustomProviderForm: React.FC<CustomFormProps> = ({
  form,
  errors,
  keyDraft,
  busy,
  onChange,
  onKeyChange,
  onSubmit,
  onCancel,
}) => {
  const set = (patch: Partial<CustomForm>) => onChange({ ...form, ...patch });
  const editing = form.id !== undefined;
  const local = isLocalUrl(form.baseUrl);
  return (
    <form
      className="settings-card hub-form"
      aria-labelledby="hub-custom-title"
      noValidate
      onSubmit={(e) => {
        e.preventDefault();
        onSubmit();
      }}
    >
      <div className="hub-form-head">
        <span className="hub-form-mark" aria-hidden="true">
          +
        </span>
        <div className="hub-form-title">
          <strong id="hub-custom-title">
            {editing ? `Edit ${form.name || "provider"}` : "Make your own"}
          </strong>
          <span className="settings-hint">
            Any OpenAI-compatible endpoint: your server, a gateway, or Ollama on
            this Mac.
          </span>
        </div>
      </div>
      <div className="hub-form-grid">
        <Field id="hub-c-cap" label="Used for">
          {(d) => (
            <select
              id="hub-c-cap"
              className="settings-input"
              value={form.capability}
              aria-describedby={d}
              onChange={(e) =>
                set({ capability: e.target.value as CustomForm["capability"] })
              }
            >
              <option value="stt">Speech to text</option>
              <option value="llm">Language model</option>
              <option value="tts">Voice (TTS)</option>
            </select>
          )}
        </Field>
        <Field id="hub-c-name" label="Name" error={errors.name}>
          {(d) => (
            <input
              id="hub-c-name"
              className="settings-input"
              type="text"
              maxLength={40}
              value={form.name}
              aria-invalid={errors.name ? true : undefined}
              aria-describedby={d}
              onChange={(e) => set({ name: e.target.value })}
            />
          )}
        </Field>
        <Field
          id="hub-c-url"
          label="Base URL"
          error={errors.baseUrl}
          hint={
            errors.baseUrl
              ? undefined
              : form.baseUrl
                ? local
                  ? "On this Mac: nothing leaves the device."
                  : "Cloud: your text is sent to this server."
                : "For example https://api.openai.com/v1"
          }
        >
          {(d) => (
            <input
              id="hub-c-url"
              className="settings-input"
              type="url"
              inputMode="url"
              autoComplete="off"
              spellCheck={false}
              value={form.baseUrl}
              aria-invalid={errors.baseUrl ? true : undefined}
              aria-describedby={d}
              onChange={(e) => set({ baseUrl: e.target.value })}
            />
          )}
        </Field>
        <Field id="hub-c-model" label="Model" error={errors.model}>
          {(d) => (
            <input
              id="hub-c-model"
              className="settings-input"
              type="text"
              autoComplete="off"
              spellCheck={false}
              maxLength={80}
              value={form.model}
              aria-invalid={errors.model ? true : undefined}
              aria-describedby={d}
              onChange={(e) => set({ model: e.target.value })}
            />
          )}
        </Field>
        <Field id="hub-c-auth" label="How the key is sent">
          {(d) => (
            <select
              id="hub-c-auth"
              className="settings-input"
              value={form.authStyle}
              aria-describedby={d}
              onChange={(e) =>
                set({ authStyle: e.target.value as CustomForm["authStyle"] })
              }
            >
              <option value="bearer">Authorization: Bearer (default)</option>
              <option value="token">Authorization: Token</option>
              <option value="x-api-key">x-api-key header</option>
              <option value="custom">Custom header</option>
              <option value="none">No key (server on this Mac)</option>
            </select>
          )}
        </Field>
        {form.authStyle === "custom" && (
          <Field
            id="hub-c-header"
            label="Header name"
            error={errors.headerName}
          >
            {(d) => (
              <input
                id="hub-c-header"
                className="settings-input"
                type="text"
                autoComplete="off"
                spellCheck={false}
                maxLength={64}
                value={form.headerName}
                aria-invalid={errors.headerName ? true : undefined}
                aria-describedby={d}
                onChange={(e) => set({ headerName: e.target.value })}
              />
            )}
          </Field>
        )}
        {form.authStyle !== "none" && (
          <Field
            id="hub-c-key"
            label="API key"
            hint={
              editing
                ? "Leave empty to keep the saved key."
                : "Saved in the macOS Keychain, never in a file."
            }
          >
            {(d) => (
              <input
                id="hub-c-key"
                className="settings-input"
                type="password"
                autoComplete="off"
                spellCheck={false}
                value={keyDraft}
                aria-describedby={d}
                onChange={(e) => onKeyChange(e.target.value)}
              />
            )}
          </Field>
        )}
        <Field id="hub-c-protocol" label="Protocol">
          {(d) => (
            <select
              id="hub-c-protocol"
              className="settings-input"
              value="openai-compatible"
              aria-describedby={d}
              onChange={() => {}}
            >
              <option value="openai-compatible">OpenAI-compatible</option>
            </select>
          )}
        </Field>
      </div>
      <div className="hub-actions">
        <button
          type="submit"
          className="settings-btn settings-btn-primary hub-btn"
          disabled={busy}
        >
          {editing ? "Save changes" : "Save"}
        </button>
        {(editing || form !== emptyForm) && (
          <button
            type="button"
            className="settings-btn hub-btn"
            onClick={onCancel}
          >
            {editing ? "Cancel" : "Clear"}
          </button>
        )}
        <span className="settings-hint">
          After saving, use Test connection below.
        </span>
      </div>
    </form>
  );
};

// ---------- your providers ----------

export interface CustomListProps {
  items: ProviderStatus[];
  results: Record<string, TestResult | "running">;
  confirmRemove: string | null;
  onTest: (id: string) => void;
  onEdit: (p: ProviderStatus) => void;
  onRemove: (id: string) => void;
}

export const CustomList: React.FC<CustomListProps> = ({
  items,
  results,
  confirmRemove,
  onTest,
  onEdit,
  onRemove,
}) => {
  if (items.length === 0)
    return (
      <div className="hub-empty">
        <span className="hub-empty-mark" aria-hidden="true">
          +
        </span>
        <p className="settings-hint">You have not added your own yet.</p>
        <p className="settings-hint">
          Point any OpenAI-compatible endpoint at DeepAI Voice.
        </p>
      </div>
    );
  return (
    <ul className="hub-list" aria-label="Your providers">
      {items.map((p) => {
        const r = results[p.id];
        const line =
          r === "running"
            ? { label: "Testing...", tone: "muted" as const }
            : r
              ? testLine(r)
              : null;
        const kind =
          p.kind === "stt" || p.kind === "llm" || p.kind === "tts"
            ? CAPABILITY_LABEL[p.kind]
            : p.kind;
        return (
          <li className="settings-card hub-card" key={p.id}>
            <div className="hub-card-head">
              <div className="hub-card-title">
                <strong>{p.name ?? p.id}</strong>
                <span className="settings-hint hub-url">
                  {p.baseUrl} · {p.model}
                </span>
              </div>
              <StatusPill
                chip={
                  p.connected
                    ? { label: "Connected", tone: "ok" }
                    : { label: "Key missing", tone: "warn" }
                }
              />
            </div>
            <ul className="hub-chips" aria-label="Details">
              <li className="hub-chip">{kind}</li>
              <li className="hub-chip">
                {p.key === "not_needed"
                  ? "No key"
                  : p.key === "set"
                    ? `Key ${maskTail(p.keyTail)}`
                    : "No key saved"}
              </li>
              <li className="hub-chip">
                {p.baseUrl && isLocalUrl(p.baseUrl) ? "On this Mac" : "Cloud"}
              </li>
            </ul>
            <div className="hub-actions">
              <button
                type="button"
                className="settings-btn hub-btn"
                disabled={r === "running"}
                onClick={() => onTest(p.id)}
              >
                Test connection
              </button>
              <button
                type="button"
                className="settings-btn hub-btn"
                onClick={() => onEdit(p)}
              >
                Edit
              </button>
              <button
                type="button"
                className="settings-btn hub-btn hub-danger"
                onClick={() => onRemove(p.id)}
              >
                {confirmRemove === p.id ? "Confirm remove" : "Remove"}
              </button>
            </div>
            <p
              className="hub-test"
              role="status"
              aria-live="polite"
              data-tone={line?.tone}
            >
              {line && (
                <>
                  <span className="perm-dot" aria-hidden="true" />
                  {line.label}
                </>
              )}
            </p>
          </li>
        );
      })}
    </ul>
  );
};

// ---------- container ----------

export interface ProvidersHubViewProps {
  catalogCards: ReturnType<typeof buildSections>;
  custom: ProviderStatus[];
  loading: boolean;
  message: Chip | null;
  connecting: string | null;
  keyDraft: string;
  busy: boolean;
  form: CustomForm;
  formErrors: FormErrors;
  formKey: string;
  results: Record<string, TestResult | "running">;
  confirmRemove: string | null;
  card: Omit<CardProps, "card" | "connecting" | "keyDraft" | "busy">;
  formHandlers: Pick<
    CustomFormProps,
    "onChange" | "onKeyChange" | "onSubmit" | "onCancel"
  >;
  list: Pick<CustomListProps, "onTest" | "onEdit" | "onRemove">;
}

/** Stateless view of the whole tab, used by the container and the tests. */
export const ProvidersHubView: React.FC<ProvidersHubViewProps> = (p) => (
  <div className="hub">
    <p className="settings-text hub-intro">{INTRO}</p>
    <p
      className="hub-live"
      role="status"
      aria-live="polite"
      data-tone={p.message?.tone}
    >
      {p.message?.label}
    </p>
    {p.loading && <p className="settings-hint">Loading providers...</p>}
    {p.catalogCards.map((s) => (
      <section
        key={s.capability}
        className="hub-section"
        data-cap={s.capability}
        aria-labelledby={`hub-sec-${s.capability}`}
      >
        <div className="hub-section-head">
          <h4 className="hub-section-title" id={`hub-sec-${s.capability}`}>
            {s.title}
          </h4>
          {s.cards.length > 0 && (
            <span className="hub-count">
              {s.cards.length} provider{s.cards.length !== 1 ? "s" : ""}
            </span>
          )}
        </div>
        {s.cards.length === 0 ? (
          <p className="settings-hint">Available in the app.</p>
        ) : (
          <div className="hub-grid">
            {s.cards.map((c) => (
              <ProviderCard
                key={c.entry.id}
                card={c}
                connecting={p.connecting === c.entry.id}
                keyDraft={p.connecting === c.entry.id ? p.keyDraft : ""}
                busy={p.busy}
                {...p.card}
              />
            ))}
          </div>
        )}
      </section>
    ))}
    <section className="hub-section" aria-labelledby="hub-sec-custom">
      <h4 className="hub-section-title" id="hub-sec-custom">
        Your own providers
      </h4>
      <CustomProviderForm
        form={p.form}
        errors={p.formErrors}
        keyDraft={p.formKey}
        busy={p.busy}
        {...p.formHandlers}
      />
      <CustomList
        items={p.custom}
        results={p.results}
        confirmRemove={p.confirmRemove}
        {...p.list}
      />
    </section>
  </div>
);

/** Settings > Providers. */
export const ProvidersHub: React.FC<{ settingsProviders?: Provider[] }> = ({
  settingsProviders = [],
}) => {
  const { catalog, statuses, loading, refresh, api, preview } = useProviders();
  const [connecting, setConnecting] = useState<string | null>(
    preview?.connecting ?? null,
  );
  const [keyDraft, setKeyDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<Chip | null>(null);
  const [form, setForm] = useState<CustomForm>(preview?.form ?? emptyForm);
  const [showErrors, setShowErrors] = useState(preview?.form !== undefined);
  const [formKey, setFormKey] = useState("");
  const [results, setResults] = useState<
    Record<string, TestResult | "running">
  >(preview?.results ?? {});
  const [confirmRemove, setConfirmRemove] = useState<string | null>(null);

  const say = (label: string, tone: Chip["tone"] = "muted") =>
    setMessage({ label, tone });
  const run = async (f: () => Promise<void>) => {
    setBusy(true);
    try {
      await f();
    } finally {
      setBusy(false);
    }
  };
  const nameOf = (id: string) =>
    catalog.find((e) => e.id === id)?.name ??
    statuses.find((s) => s.id === id)?.name ??
    id;

  const showForm = () =>
    document
      .getElementById("hub-sec-custom")
      ?.scrollIntoView({ block: "start", behavior: "smooth" });

  const resetForm = () => {
    setForm(emptyForm);
    setFormKey("");
    setShowErrors(false);
  };

  const formErrors = showErrors ? validateForm(form) : {};

  return (
    <ProvidersHubView
      catalogCards={buildSections(catalog, statuses, settingsProviders)}
      custom={customProviders(statuses)}
      loading={loading}
      message={message}
      connecting={connecting}
      keyDraft={keyDraft}
      busy={busy}
      form={form}
      formErrors={formErrors}
      formKey={formKey}
      results={results}
      confirmRemove={confirmRemove}
      card={{
        onConnect: (id) => {
          setConnecting(id);
          setKeyDraft("");
        },
        onKeyChange: setKeyDraft,
        onCancel: () => {
          setConnecting(null);
          setKeyDraft("");
        },
        onSaveKey: (id) =>
          void run(async () => {
            const key = keyDraft;
            setKeyDraft("");
            const r = await api.connect(id, key);
            if (r.ok) {
              setConnecting(null);
              say(`${nameOf(id)} connected.`, "ok");
              refresh();
            } else say(r.error, "error");
          }),
        onDisconnect: (id) =>
          void run(async () => {
            const r = await api.disconnect(id);
            if (r.ok) {
              say(`${nameOf(id)} disconnected. Its key was removed.`);
              refresh();
            } else say(r.error, "error");
          }),
        onDocs: (id) => void api.openDocs(id),
        onSetup: (card) => {
          const e = card.entry;
          setForm({
            ...emptyForm,
            capability: e.capability,
            name: e.id === "ollama" ? "Ollama" : "",
            baseUrl: e.baseUrlDefault ?? "",
            authStyle: e.authStyle === "none" ? "none" : "bearer",
          });
          setShowErrors(false);
          showForm();
          document.getElementById("hub-c-name")?.focus();
        },
      }}
      formHandlers={{
        onChange: setForm,
        onKeyChange: setFormKey,
        onCancel: resetForm,
        onSubmit: () =>
          void run(async () => {
            setShowErrors(true);
            if (Object.keys(validateForm(form)).length > 0) {
              say("Check the highlighted fields.", "error");
              return;
            }
            const key = formKey;
            setFormKey("");
            const r = await api.saveCustom(form, key);
            if (r.ok) {
              say(`${r.value.name ?? "Provider"} saved.`, "ok");
              resetForm();
              refresh();
            } else say(r.error, "error");
          }),
      }}
      list={{
        onTest: (id) => {
          setResults((m) => ({ ...m, [id]: "running" }));
          void api.test(id).then((r) =>
            setResults((m) => ({
              ...m,
              [id]: r.ok
                ? r.value
                : { result: "not_available", reason: r.error },
            })),
          );
        },
        onEdit: (p) => {
          setForm({
            id: p.id,
            capability: p.kind === "stt" || p.kind === "tts" ? p.kind : "llm",
            name: p.name ?? "",
            baseUrl: p.baseUrl ?? "",
            model: p.model,
            ...authFromSetting(p.authStyle),
          });
          setFormKey("");
          setShowErrors(false);
          showForm();
        },
        onRemove: (id) => {
          if (confirmRemove !== id) {
            setConfirmRemove(id);
            return;
          }
          setConfirmRemove(null);
          void run(async () => {
            const r = await api.deleteCustom(id);
            if (r.ok) {
              say(`${nameOf(id)} removed. Its key was deleted.`);
              refresh();
            } else say(r.error, "error");
          });
        },
      }}
    />
  );
};
