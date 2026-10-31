import { Settings } from "./types";

export type StatusKind = "ok" | "error";

export interface SettingsState {
  /** Values last loaded or saved. Reset returns here. */
  baseline: Settings;
  /** Current editable values. */
  settings: Settings;
  /** Typed API keys by provider id. Never persisted, cleared on save or reset. */
  keyDrafts: Record<string, string>;
  /** Last 4 chars of a key saved in this session, by provider id. */
  keyHints: Record<string, string>;
  loading: boolean;
  loadError: string | null;
  status: { kind: StatusKind; text: string } | null;
}

export type SettingsAction =
  | { type: "loaded"; settings: Settings }
  | { type: "loadFailed"; message: string }
  | { type: "edit"; settings: Settings }
  | { type: "editKey"; providerId: string; value: string }
  | {
      type: "saved";
      settings: Settings;
      keys: Record<string, string>;
      hints: Record<string, string>;
    }
  | { type: "saveFailed"; message: string }
  | { type: "reset" };

export function initialState(
  initial: Settings,
  loading: boolean,
): SettingsState {
  return {
    baseline: initial,
    settings: initial,
    keyDrafts: {},
    keyHints: {},
    loading,
    loadError: null,
    status: null,
  };
}

export function hasKeyDraft(state: SettingsState): boolean {
  return Object.values(state.keyDrafts).some((v) => v.length > 0);
}

export function isDirty(state: SettingsState): boolean {
  return (
    hasKeyDraft(state) ||
    JSON.stringify(state.settings) !== JSON.stringify(state.baseline)
  );
}

/** Last 4 characters of a key, or "" when the key is too short to reveal any. */
export function keyTail(key: string): string {
  return key.length >= 8 ? key.slice(-4) : "";
}

export function maskKey(tail: string): string {
  return `••••••••${tail}`;
}

export function reducer(
  state: SettingsState,
  action: SettingsAction,
): SettingsState {
  switch (action.type) {
    case "loaded":
      return {
        ...state,
        baseline: action.settings,
        settings: action.settings,
        keyDrafts: {},
        loading: false,
        loadError: null,
      };
    case "loadFailed":
      return { ...state, loading: false, loadError: action.message };
    case "edit":
      return { ...state, settings: action.settings, status: null };
    case "editKey":
      return {
        ...state,
        keyDrafts: { ...state.keyDrafts, [action.providerId]: action.value },
        status: null,
      };
    case "saved":
      return {
        ...state,
        baseline: action.settings,
        // Keep edits typed while the save was running; clear only the keys that were saved.
        keyDrafts: Object.fromEntries(
          Object.entries(state.keyDrafts).filter(
            ([id, v]) => action.keys[id] !== v,
          ),
        ),
        keyHints: { ...state.keyHints, ...action.hints },
        status: { kind: "ok", text: "Settings saved" },
      };
    case "saveFailed":
      return { ...state, status: { kind: "error", text: action.message } };
    case "reset":
      return {
        ...state,
        settings: state.baseline,
        keyDrafts: {},
        status: { kind: "ok", text: "Changes discarded" },
      };
  }
}
