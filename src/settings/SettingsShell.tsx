import React, { useEffect, useReducer, useRef, useState } from "react";
import "./settings.css";
import { Settings, SettingsTab, defaultSettings } from "./types";
import { initialState, isDirty, keyTail, reducer } from "./state";
import {
  AboutPanel,
  GeneralPanel,
  PrivacyPanel,
  ShortcutsPanel,
  StylesPanel,
} from "./Panels";
import { PermissionsPanel } from "./PermissionsPanel";
import { LaunchAtLogin } from "./LaunchAtLogin";
import { ProvidersHub } from "./providers";

export interface SettingsShellProps {
  initialSettings?: Settings;
  /** Tab shown first. */
  activeTab?: SettingsTab;
  /** Injected loader (Tauri command wrapper later). */
  loadSettings?: () => Promise<Settings>;
  /** Receives settings and typed API keys by provider id. Keys are not kept after. */
  onSave?: (
    settings: Settings,
    apiKeys: Record<string, string>,
  ) => void | Promise<void>;
  onReset?: () => void;
  /** Tabs to show, in order. Defaults to all. One tab renders without a tab bar. */
  tabs?: SettingsTab[];
}

const allTabs: { id: SettingsTab; label: string }[] = [
  { id: "general", label: "General" },
  { id: "permissions", label: "Permissions" },
  { id: "providers", label: "Providers" },
  { id: "shortcuts", label: "Shortcuts" },
  { id: "styles", label: "Styles" },
  { id: "privacy", label: "Privacy" },
  { id: "about", label: "About" },
];

export const SettingsShell: React.FC<SettingsShellProps> = ({
  initialSettings = defaultSettings,
  activeTab = "general",
  loadSettings,
  onSave,
  onReset,
  tabs: tabIds,
}) => {
  const tabs = tabIds ? allTabs.filter((t) => tabIds.includes(t.id)) : allTabs;
  const single = tabs.length === 1;
  const [state, dispatch] = useReducer(
    reducer,
    initialState(initialSettings, loadSettings !== undefined),
  );
  const [active, setActive] = useState<SettingsTab>(
    tabs.some((t) => t.id === activeTab) ? activeTab : tabs[0].id,
  );
  const tabRefs = useRef<Record<string, HTMLButtonElement | null>>({});

  useEffect(() => {
    if (!loadSettings) return;
    let cancelled = false;
    loadSettings().then(
      (s) => !cancelled && dispatch({ type: "loaded", settings: s }),
      (e: unknown) =>
        !cancelled &&
        dispatch({
          type: "loadFailed",
          message: e instanceof Error ? e.message : "Could not load settings",
        }),
    );
    return () => {
      cancelled = true;
    };
  }, [loadSettings]);

  const dirty = isDirty(state);

  const savingRef = useRef(false);
  const [saving, setSaving] = useState(false);

  const handleSave = async () => {
    if (savingRef.current) return;
    savingRef.current = true;
    setSaving(true);
    const snapshot = state.settings;
    const keys: Record<string, string> = {};
    const hints: Record<string, string> = {};
    for (const [id, v] of Object.entries(state.keyDrafts)) {
      if (v.length > 0) {
        keys[id] = v;
        const tail = keyTail(v);
        if (tail) hints[id] = tail;
      }
    }
    try {
      await onSave?.(snapshot, keys);
      dispatch({ type: "saved", settings: snapshot, keys, hints });
    } catch {
      dispatch({ type: "saveFailed", message: "Could not save settings" });
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  };

  const onTabKey = (e: React.KeyboardEvent, index: number) => {
    const last = tabs.length - 1;
    let next = -1;
    if (e.key === "ArrowDown" || e.key === "ArrowRight")
      next = index === last ? 0 : index + 1;
    else if (e.key === "ArrowUp" || e.key === "ArrowLeft")
      next = index === 0 ? last : index - 1;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = last;
    if (next < 0) return;
    e.preventDefault();
    setActive(tabs[next].id);
    tabRefs.current[tabs[next].id]?.focus();
  };

  const change = (settings: Settings) => dispatch({ type: "edit", settings });
  const common = { settings: state.settings, onChange: change };

  let body: React.ReactNode;
  if (state.loading) {
    body = (
      <p className="settings-message" role="status">
        Loading settings...
      </p>
    );
  } else if (state.loadError) {
    body = (
      <p className="settings-message" role="alert">
        {state.loadError}
      </p>
    );
  } else {
    body = (
      <div
        className="settings-panel"
        role={single ? undefined : "tabpanel"}
        id={`panel-${active}`}
        aria-labelledby={single ? undefined : `tab-${active}`}
      >
        {!single && (
          <h3 className="settings-heading">
            {tabs.find((t) => t.id === active)?.label}
          </h3>
        )}
        {active === "general" && <LaunchAtLogin />}
        {active === "general" && <GeneralPanel {...common} />}
        {active === "permissions" && <PermissionsPanel />}
        {active === "providers" && (
          <ProvidersHub settingsProviders={state.settings.providers} />
        )}
        {active === "shortcuts" && <ShortcutsPanel {...common} />}
        {active === "styles" && <StylesPanel {...common} />}
        {active === "privacy" && <PrivacyPanel />}
        {active === "about" && <AboutPanel />}
      </div>
    );
  }

  return (
    <div className="settings-root">
      {!single && (
        <div className="settings-tabbar">
          <div
            role="tablist"
            aria-label="Settings"
            aria-orientation="horizontal"
            className="settings-tablist"
          >
            {tabs.map((tab, i) => (
              <button
                key={tab.id}
                ref={(el) => {
                  tabRefs.current[tab.id] = el;
                }}
                type="button"
                role="tab"
                className="settings-tab"
                id={`tab-${tab.id}`}
                aria-selected={active === tab.id}
                aria-controls={`panel-${tab.id}`}
                tabIndex={active === tab.id ? 0 : -1}
                onClick={() => setActive(tab.id)}
                onKeyDown={(e) => onTabKey(e, i)}
              >
                {tab.label}
              </button>
            ))}
          </div>
        </div>
      )}
      <div className="settings-main">
        {body}
        <footer className="settings-footer">
          <span
            className="settings-status"
            role="status"
            data-kind={state.status?.kind}
          >
            {state.status?.text ?? (dirty ? "Unsaved changes" : "")}
          </span>
          <button
            type="button"
            className="settings-btn"
            disabled={!dirty}
            onClick={() => {
              dispatch({ type: "reset" });
              onReset?.();
            }}
          >
            Reset
          </button>
          <button
            type="button"
            className="settings-btn settings-btn-primary"
            disabled={!dirty || saving}
            onClick={handleSave}
          >
            Save
          </button>
        </footer>
      </div>
    </div>
  );
};
