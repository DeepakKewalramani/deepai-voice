import React from "react";
import { PermissionKind, PermissionsStatus, buildRows } from "./permissions";
import { usePermissions } from "./usePermissions";

const Chevron: React.FC = () => (
  <svg
    className="perm-chevron"
    width="8"
    height="12"
    viewBox="0 0 8 12"
    fill="none"
    aria-hidden="true"
  >
    <path
      d="M1.5 1.5 6 6l-4.5 4.5"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

export interface PermissionsListProps {
  status: PermissionsStatus;
  onOpen: (kind: PermissionKind) => void;
  onRequest: (kind: PermissionKind) => void;
}

/** The permission rows. Each row opens its System Settings pane. */
export const PermissionsList: React.FC<PermissionsListProps> = ({
  status,
  onOpen,
  onRequest,
}) => (
  <div className="perm-list">
    <p className="settings-text">
      Click a row to open it in System Settings. The status updates on its own.
    </p>
    {buildRows(status).map((row) => (
      <div className="settings-card perm-card" key={row.kind}>
        <button
          type="button"
          className="perm-row"
          title={`Open ${row.title} in System Settings`}
          onClick={() => onOpen(row.kind)}
        >
          <span className="perm-text">
            <strong className="perm-title">{row.title}</strong>
            <span className="settings-hint">{row.why}</span>
          </span>
          <span className="perm-pill" data-tone={row.pill.tone}>
            <span className="perm-dot" aria-hidden="true" />
            {row.pill.label}
          </span>
          <Chevron />
        </button>
        {row.showRequest && (
          <div className="perm-actions">
            <button
              type="button"
              className="settings-btn perm-request"
              onClick={() => onRequest(row.kind)}
            >
              Request access
            </button>
          </div>
        )}
      </div>
    ))}
  </div>
);

/** Settings > Permissions. Polls only while mounted (the tab is visible). */
export const PermissionsPanel: React.FC = () => {
  const { status, open, request } = usePermissions({ poll: true });
  return <PermissionsList status={status} onOpen={open} onRequest={request} />;
};
