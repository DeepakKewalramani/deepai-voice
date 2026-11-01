/** Launch at login state for the General tab. */
export interface AutostartState {
  /** Null until the status is read. */
  enabled: boolean | null;
  busy: boolean;
  error: string | null;
}

export const initialAutostart: AutostartState = {
  enabled: null,
  busy: false,
  error: null,
};

export type AutostartAction =
  | { type: "loaded"; enabled: boolean }
  | { type: "toggle"; enabled: boolean }
  | { type: "saved" }
  | { type: "failed"; previous: boolean | null; message: string };

export function autostartReducer(
  state: AutostartState,
  action: AutostartAction,
): AutostartState {
  switch (action.type) {
    case "loaded":
      return { enabled: action.enabled, busy: false, error: null };
    case "toggle":
      return { enabled: action.enabled, busy: true, error: null };
    case "saved":
      return { ...state, busy: false };
    case "failed":
      return { enabled: action.previous, busy: false, error: action.message };
  }
}

type Invoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

export const NO_BACKEND =
  "Launch at login is available only in the installed app.";

/** Reads the status. A missing or failing backend reads as off. */
export async function loadAutostart(
  invokeFn: Invoke | undefined,
): Promise<boolean> {
  if (!invokeFn) return false;
  try {
    return (await invokeFn("autostart_status")) === true;
  } catch {
    return false;
  }
}

/** Saves the choice. Null on success, otherwise a message to show. */
export async function saveAutostart(
  invokeFn: Invoke | undefined,
  enabled: boolean,
): Promise<string | null> {
  if (!invokeFn) return NO_BACKEND;
  try {
    await invokeFn("set_autostart", { enabled });
    return null;
  } catch (e) {
    if (typeof e === "string" && e.trim() !== "") return e;
    if (e instanceof Error && e.message) return e.message;
    return "Could not change launch at login.";
  }
}
