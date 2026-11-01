import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  PermissionKind,
  PermissionsStatus,
  UNKNOWN_STATUS,
  devPreviewStatus,
  parseStatus,
} from "./permissions";

/** How often the Permissions tab re-reads the status while it is visible. */
export const POLL_MS = 2000;

type Invoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Reads the status through `invoke`. Missing or failing backend reads unknown. */
export async function loadPermissions(
  invokeFn: Invoke | undefined,
): Promise<PermissionsStatus> {
  if (!invokeFn) return UNKNOWN_STATUS;
  try {
    return parseStatus(await invokeFn("permissions_status"));
  } catch {
    return UNKNOWN_STATUS;
  }
}

function initialStatus(): PermissionsStatus {
  if (import.meta.env.DEV && typeof window !== "undefined" && !inTauri())
    return devPreviewStatus(window.location.search) ?? UNKNOWN_STATUS;
  return UNKNOWN_STATUS;
}

export interface UsePermissions {
  status: PermissionsStatus;
  open: (kind: PermissionKind) => void;
  request: (kind: PermissionKind) => void;
}

/**
 * macOS permission status. Re-reads when the window regains focus and, with
 * `poll`, every 2 s while the page is visible. Outside Tauri it stays unknown
 * (or the dev `?perm=` preview).
 */
export function usePermissions({ poll }: { poll: boolean }): UsePermissions {
  const [status, setStatus] = useState<PermissionsStatus>(initialStatus);

  const refresh = useCallback(() => {
    void loadPermissions(invoke).then(setStatus);
  }, []);

  useEffect(() => {
    if (!inTauri()) return;
    refresh();
    let timer: ReturnType<typeof setInterval> | undefined;
    const stop = () => {
      if (timer !== undefined) clearInterval(timer);
      timer = undefined;
    };
    const start = () => {
      if (poll && timer === undefined && !document.hidden)
        timer = setInterval(refresh, POLL_MS);
    };
    const onFocus = () => {
      refresh();
      start();
    };
    const onVisibility = () => {
      if (document.hidden) stop();
      else onFocus();
    };
    start();
    window.addEventListener("focus", onFocus);
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      stop();
      window.removeEventListener("focus", onFocus);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [poll, refresh]);

  const open = useCallback((kind: PermissionKind) => {
    if (!inTauri()) return;
    invoke("open_permission_pane", { kind }).catch(() => {
      // Nothing to show: the row stays as it is.
    });
  }, []);

  const request = useCallback(
    (kind: PermissionKind) => {
      if (!inTauri()) return;
      invoke("request_permission", { kind }).then(refresh, refresh);
    },
    [refresh],
  );

  return { status, open, request };
}
