import React from "react";
import "./status-card.css";
import { DictationKind, DictationStatus, liveLine } from "./status";

const copy: Record<
  DictationKind,
  { pill: string; title: string; detail: string }
> = {
  ready: {
    pill: "Ready",
    title: "Hold Fn to dictate",
    detail: "Speak, then release. The text is pasted where your cursor is.",
  },
  recording: {
    pill: "Recording",
    title: "Listening",
    detail: "Release the keys to stop and transcribe.",
  },
  transcribing: {
    pill: "Transcribing",
    title: "Transcribing",
    detail: "Turning your speech into text.",
  },
  error: {
    pill: "Error",
    title: "Dictation stopped",
    detail: "Something went wrong.",
  },
};

const Keys: React.FC<{ keys: string[] }> = ({ keys }) => (
  <span className="keys">
    {keys.map((k) => (
      <kbd key={k} className="keycap">
        {k}
      </kbd>
    ))}
  </span>
);

const Mark: React.FC<{ kind: DictationKind }> = ({ kind }) => (
  <div className="status-mark" aria-hidden="true">
    {kind === "error" ? (
      <svg width="26" height="26" viewBox="0 0 20 20" fill="none">
        <path
          d="M10 6v5M10 14h.01"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
        />
      </svg>
    ) : (
      <svg width="26" height="26" viewBox="0 0 20 20" fill="none">
        <rect x="7" y="2.5" width="6" height="10" rx="3" fill="currentColor" />
        <path
          d="M4.5 9.5a5.5 5.5 0 0 0 11 0M10 15v2.5"
          stroke="currentColor"
          strokeWidth="1.6"
          strokeLinecap="round"
        />
      </svg>
    )}
  </div>
);

export interface StatusCardProps {
  status: DictationStatus;
  /** Live, not yet typed text while recording. */
  partial?: string;
  /** Set when a required macOS permission is missing; opens Settings > Permissions. */
  onFixPermissions?: () => void;
}

export const StatusCard: React.FC<StatusCardProps> = ({
  status,
  partial = "",
  onFixPermissions,
}) => {
  const c = copy[status.kind];
  const live = liveLine(status, partial);
  const detail =
    status.kind === "error" && status.message ? status.message : c.detail;
  return (
    <section
      className="status-card"
      data-state={status.kind}
      aria-label="Dictation status"
    >
      <div className="status-body">
        <span className="status-pill" role="status" aria-live="polite">
          <span className="status-dot" aria-hidden="true" />
          {c.pill}
        </span>
        <h3 className="status-title">{c.title}</h3>
        <p className="status-detail">{detail}</p>
        {live !== "" && (
          <p className="status-live">
            <span className="status-live-label">Live</span>
            <span className="status-live-text">{live}</span>
          </p>
        )}
        <p className="status-hint">
          <span className="status-hint-label">Shortcut</span>
          <Keys keys={["Fn"]} />
        </p>
        {onFixPermissions && (
          <p className="status-perm">
            Permissions needed.{" "}
            <button
              type="button"
              className="status-perm-link"
              onClick={onFixPermissions}
            >
              Open Settings
            </button>
          </p>
        )}
      </div>
      <Mark kind={status.kind} />
    </section>
  );
};
