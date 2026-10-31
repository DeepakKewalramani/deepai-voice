import React from "react";
import { Settings } from "./types";
import { maskKey } from "./state";

interface PanelProps {
  settings: Settings;
  onChange: (next: Settings) => void;
}

export const GeneralPanel: React.FC<PanelProps> = ({ settings, onChange }) => (
  <div className="settings-card settings-field">
    <label className="settings-label" htmlFor="max-duration">
      Maximum recording duration (seconds)
    </label>
    <input
      id="max-duration"
      className="settings-input"
      type="number"
      min={1}
      max={600}
      value={settings.maxRecordSeconds}
      onChange={(e) =>
        onChange({
          ...settings,
          maxRecordSeconds: Math.max(1, Number(e.target.value) || 1),
        })
      }
    />
    <p className="settings-hint">120 seconds default cap to conserve memory</p>
  </div>
);

interface ProvidersProps extends PanelProps {
  keyDrafts: Record<string, string>;
  keyHints: Record<string, string>;
  onKeyChange: (providerId: string, value: string) => void;
}

export const ProvidersPanel: React.FC<ProvidersProps> = ({
  settings,
  keyDrafts,
  keyHints,
  onKeyChange,
}) => (
  <div>
    {settings.providers.map((p) => (
      <div className="settings-card" key={p.id}>
        <div className="settings-row">
          <div>
            <strong>
              {p.id === "whisper-local"
                ? "Local Whisper (whisper-local)"
                : p.id}
            </strong>
            <p className="settings-hint">
              Kind: {p.kind.toUpperCase()} | Adapter: {p.adapter} | Model:{" "}
              {p.model}
            </p>
          </div>
          <span className="settings-badge">
            {p.keyRef ? "Cloud" : "Device"}
          </span>
        </div>
        {p.keyRef && (
          <div className="settings-key">
            <label className="settings-label" htmlFor={`key-${p.id}`}>
              API key for {p.id}
            </label>
            <input
              id={`key-${p.id}`}
              className="settings-input"
              type="password"
              autoComplete="off"
              spellCheck={false}
              value={keyDrafts[p.id] ?? ""}
              onChange={(e) => onKeyChange(p.id, e.target.value)}
            />
            <p className="settings-hint" data-testid={`key-mask-${p.id}`}>
              Stored key:{" "}
              {keyHints[p.id] ? maskKey(keyHints[p.id]) : "not shown"}
            </p>
          </div>
        )}
      </div>
    ))}
  </div>
);

export const ShortcutsPanel: React.FC<PanelProps> = ({
  settings,
  onChange,
}) => (
  <div className="settings-card settings-field">
    <label className="settings-label" htmlFor="dictate-key">
      Dictation shortcut
    </label>
    <input
      id="dictate-key"
      className="settings-input"
      type="text"
      value={settings.hotkeys.dictate}
      onChange={(e) =>
        onChange({ ...settings, hotkeys: { dictate: e.target.value } })
      }
    />
    <p className="settings-hint">
      Hold Fn (Globe) key or tap fallback combination to record.
    </p>
  </div>
);

export const StylesPanel: React.FC<PanelProps> = ({ settings, onChange }) => (
  <div>
    <div className="settings-card settings-field">
      <label className="settings-label" htmlFor="active-style">
        Active style
      </label>
      <select
        id="active-style"
        className="settings-input"
        value={settings.activeStyle}
        onChange={(e) => onChange({ ...settings, activeStyle: e.target.value })}
      >
        {settings.styles.map((s) => (
          <option key={s.id} value={s.id}>
            {s.name}
          </option>
        ))}
      </select>
    </div>
    <div className="settings-list">
      {settings.styles.map((s) => (
        <div className="settings-card" key={s.id}>
          <strong>{s.name}</strong>
          <p className="settings-text">{s.systemPrompt}</p>
          <p className="settings-hint">
            Template: <code className="settings-code">{s.userTemplate}</code>
          </p>
        </div>
      ))}
    </div>
  </div>
);

export const PrivacyPanel: React.FC = () => (
  <div className="settings-card">
    <p className="settings-text">
      <strong>Local processing by default:</strong> Audio recordings and
      transcripts are processed strictly in volatile memory. No audio buffers
      are saved to disk.
    </p>
  </div>
);

export const AboutPanel: React.FC = () => (
  <div className="settings-card">
    <p className="settings-text">
      DeepAI Voice v0.1.0. Menu bar dictation, translation, and meeting notes.
    </p>
  </div>
);
