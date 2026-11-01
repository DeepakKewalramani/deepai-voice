import React, { useEffect, useReducer } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  AutostartState,
  autostartReducer,
  initialAutostart,
  loadAutostart,
  saveAutostart,
} from "./autostart";

function tauriInvoke() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window
    ? invoke
    : undefined;
}

export interface LaunchAtLoginRowProps {
  state: AutostartState;
  onToggle: (enabled: boolean) => void;
}

/** "Launch at login" switch row for the General tab. */
export const LaunchAtLoginRow: React.FC<LaunchAtLoginRowProps> = ({
  state,
  onToggle,
}) => (
  <div className="settings-card">
    <div className="settings-row switch-row">
      <div>
        <strong id="autostart-label">Launch at login</strong>
        <p className="settings-hint" id="autostart-help">
          Starts DeepAI Voice when you log in, so you never open it by hand.
        </p>
      </div>
      <button
        type="button"
        role="switch"
        className="settings-switch"
        aria-checked={state.enabled === true}
        aria-labelledby="autostart-label"
        aria-describedby="autostart-help"
        disabled={state.enabled === null || state.busy}
        onClick={() => onToggle(state.enabled !== true)}
      >
        <span className="settings-switch-knob" aria-hidden="true" />
      </button>
    </div>
    {state.error && (
      <p className="settings-hint settings-error" role="alert">
        {state.error}
      </p>
    )}
  </div>
);

export const LaunchAtLogin: React.FC = () => {
  const [state, dispatch] = useReducer(autostartReducer, initialAutostart);

  useEffect(() => {
    let active = true;
    void loadAutostart(tauriInvoke()).then(
      (enabled) => active && dispatch({ type: "loaded", enabled }),
    );
    return () => {
      active = false;
    };
  }, []);

  const toggle = (enabled: boolean) => {
    const previous = state.enabled;
    dispatch({ type: "toggle", enabled });
    void saveAutostart(tauriInvoke(), enabled).then((message) =>
      dispatch(
        message === null
          ? { type: "saved" }
          : { type: "failed", previous, message },
      ),
    );
  };

  return <LaunchAtLoginRow state={state} onToggle={toggle} />;
};
