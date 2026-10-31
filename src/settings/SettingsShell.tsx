import React, { useState } from "react";
import { Settings, SettingsTab, defaultSettings } from "./types";

interface SettingsShellProps {
  initialSettings?: Settings;
  activeTab?: SettingsTab;
  onSave?: (settings: Settings) => void;
  onReset?: () => void;
}

export const SettingsShell: React.FC<SettingsShellProps> = ({
  initialSettings = defaultSettings,
  activeTab: controlledTab,
  onSave,
  onReset,
}) => {
  const [currentTab, setCurrentTab] = useState<SettingsTab>(
    controlledTab || "general",
  );
  const [settings] = useState<Settings>(initialSettings);
  const active = controlledTab !== undefined ? controlledTab : currentTab;

  const tabs: { id: SettingsTab; label: string }[] = [
    { id: "general", label: "General" },
    { id: "providers", label: "Providers" },
    { id: "shortcuts", label: "Shortcuts" },
    { id: "styles", label: "Styles" },
    { id: "privacy", label: "Privacy" },
    { id: "about", label: "About" },
  ];

  return (
    <div
      style={{
        display: "flex",
        height: "100vh",
        fontFamily:
          "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif",
        color: "#1f2937",
        backgroundColor: "#f9fafb",
      }}
    >
      {/* Sidebar Navigation */}
      <nav
        aria-label="Settings navigation"
        style={{
          width: "180px",
          backgroundColor: "#f3f4f6",
          borderRight: "1px solid #e5e7eb",
          padding: "16px 8px",
          display: "flex",
          flexDirection: "column",
          gap: "4px",
        }}
      >
        <h2
          style={{
            fontSize: "14px",
            fontWeight: "700",
            padding: "8px 12px",
            color: "#4b5563",
            textTransform: "uppercase",
            letterSpacing: "0.05em",
          }}
        >
          Settings
        </h2>
        {tabs.map((tab) => (
          <button
            key={tab.id}
            type="button"
            onClick={() => setCurrentTab(tab.id)}
            style={{
              textAlign: "left",
              padding: "8px 12px",
              borderRadius: "6px",
              border: "none",
              backgroundColor: active === tab.id ? "#ffffff" : "transparent",
              color: active === tab.id ? "#111827" : "#4b5563",
              fontWeight: active === tab.id ? "600" : "400",
              boxShadow:
                active === tab.id ? "0 1px 3px rgba(0,0,0,0.1)" : "none",
              cursor: "pointer",
              fontSize: "13px",
            }}
          >
            {tab.label}
          </button>
        ))}
      </nav>

      {/* Main Content Area */}
      <main
        style={{
          flex: 1,
          display: "flex",
          flexDirection: "column",
          overflowY: "auto",
        }}
      >
        <div style={{ flex: 1, padding: "24px 32px" }}>
          {active === "general" && (
            <section aria-labelledby="general-heading">
              <h3
                id="general-heading"
                style={{
                  fontSize: "18px",
                  fontWeight: "600",
                  marginBottom: "16px",
                }}
              >
                General Settings
              </h3>
              <div
                style={{
                  display: "flex",
                  flexDirection: "column",
                  gap: "16px",
                }}
              >
                <div>
                  <label
                    htmlFor="max-duration"
                    style={{
                      display: "block",
                      fontSize: "13px",
                      fontWeight: "500",
                      marginBottom: "4px",
                    }}
                  >
                    Maximum recording duration
                  </label>
                  <p
                    id="max-duration"
                    style={{ fontSize: "13px", color: "#6b7280" }}
                  >
                    {settings.maxRecordSeconds} seconds (120 seconds default cap
                    to conserve memory)
                  </p>
                </div>
              </div>
            </section>
          )}

          {active === "providers" && (
            <section aria-labelledby="providers-heading">
              <h3
                id="providers-heading"
                style={{
                  fontSize: "18px",
                  fontWeight: "600",
                  marginBottom: "16px",
                }}
              >
                Speech-to-Text Providers
              </h3>
              <div
                style={{
                  display: "flex",
                  flexDirection: "column",
                  gap: "12px",
                }}
              >
                {settings.providers.map((p) => (
                  <div
                    key={p.id}
                    style={{
                      padding: "12px 16px",
                      backgroundColor: "#ffffff",
                      border: "1px solid #e5e7eb",
                      borderRadius: "6px",
                    }}
                  >
                    <div
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                        alignItems: "center",
                      }}
                    >
                      <div>
                        <strong style={{ fontSize: "14px" }}>
                          {p.id === "whisper-local"
                            ? "Local Whisper (whisper-local)"
                            : p.id}
                        </strong>
                        <p
                          style={{
                            fontSize: "12px",
                            color: "#6b7280",
                            marginTop: "2px",
                          }}
                        >
                          Kind: {p.kind.toUpperCase()} | Adapter: {p.adapter} |
                          Model: {p.model}
                        </p>
                      </div>
                      <span
                        style={{
                          fontSize: "11px",
                          padding: "2px 8px",
                          backgroundColor: "#dbeafe",
                          color: "#1e40af",
                          borderRadius: "12px",
                        }}
                      >
                        {p.keyRef ? "Cloud" : "Device"}
                      </span>
                    </div>
                    {p.keyRef && (
                      <div style={{ marginTop: "8px", fontSize: "12px" }}>
                        <span style={{ color: "#4b5563" }}>API Key: </span>
                        <code style={{ letterSpacing: "2px" }}>••••••••</code>
                      </div>
                    )}
                  </div>
                ))}
              </div>
            </section>
          )}

          {active === "shortcuts" && (
            <section aria-labelledby="shortcuts-heading">
              <h3
                id="shortcuts-heading"
                style={{
                  fontSize: "18px",
                  fontWeight: "600",
                  marginBottom: "16px",
                }}
              >
                Keyboard Shortcuts
              </h3>
              <div
                style={{
                  backgroundColor: "#ffffff",
                  padding: "16px",
                  borderRadius: "6px",
                  border: "1px solid #e5e7eb",
                }}
              >
                <label
                  htmlFor="dictate-key"
                  style={{
                    display: "block",
                    fontSize: "13px",
                    fontWeight: "500",
                    marginBottom: "6px",
                  }}
                >
                  Dictation shortcut
                </label>
                <div
                  id="dictate-key"
                  style={{
                    display: "inline-block",
                    padding: "4px 8px",
                    backgroundColor: "#f3f4f6",
                    border: "1px solid #d1d5db",
                    borderRadius: "4px",
                    fontSize: "13px",
                    fontWeight: "600",
                    fontFamily: "monospace",
                  }}
                >
                  {settings.hotkeys.dictate}
                </div>
                <p
                  style={{
                    fontSize: "12px",
                    color: "#6b7280",
                    marginTop: "6px",
                  }}
                >
                  Hold Fn (Globe) key or tap fallback combination to record.
                </p>
              </div>
            </section>
          )}

          {active === "styles" && (
            <section aria-labelledby="styles-heading">
              <h3
                id="styles-heading"
                style={{
                  fontSize: "18px",
                  fontWeight: "600",
                  marginBottom: "16px",
                }}
              >
                Prompt Styles
              </h3>
              {settings.styles.map((style) => (
                <div
                  key={style.id}
                  style={{
                    backgroundColor: "#ffffff",
                    padding: "16px",
                    borderRadius: "6px",
                    border: "1px solid #e5e7eb",
                    marginBottom: "12px",
                  }}
                >
                  <strong style={{ fontSize: "14px" }}>{style.name}</strong>
                  <p
                    style={{
                      fontSize: "13px",
                      color: "#4b5563",
                      marginTop: "4px",
                    }}
                  >
                    {style.systemPrompt}
                  </p>
                  <div style={{ marginTop: "8px" }}>
                    <span style={{ fontSize: "12px", color: "#6b7280" }}>
                      Template:{" "}
                    </span>
                    <code
                      style={{
                        backgroundColor: "#f3f4f6",
                        padding: "2px 6px",
                        borderRadius: "4px",
                        fontSize: "12px",
                      }}
                    >
                      {style.userTemplate}
                    </code>
                  </div>
                </div>
              ))}
            </section>
          )}

          {active === "privacy" && (
            <section aria-labelledby="privacy-heading">
              <h3
                id="privacy-heading"
                style={{
                  fontSize: "18px",
                  fontWeight: "600",
                  marginBottom: "16px",
                }}
              >
                Privacy & Data Safety
              </h3>
              <div
                style={{
                  backgroundColor: "#ffffff",
                  padding: "16px",
                  borderRadius: "6px",
                  border: "1px solid #e5e7eb",
                }}
              >
                <p style={{ fontSize: "13px", color: "#374151" }}>
                  <strong>Local processing by default:</strong> Audio recordings
                  and transcripts are processed strictly in volatile memory. No
                  audio buffers are saved to disk.
                </p>
              </div>
            </section>
          )}

          {active === "about" && (
            <section aria-labelledby="about-heading">
              <h3
                id="about-heading"
                style={{
                  fontSize: "18px",
                  fontWeight: "600",
                  marginBottom: "16px",
                }}
              >
                About DeepAI Voice
              </h3>
              <p style={{ fontSize: "13px", color: "#4b5563" }}>
                DeepAI Voice v0.1.0 — Menu bar dictation, translation, and
                meeting notes.
              </p>
            </section>
          )}
        </div>

        {/* Footer Actions */}
        <footer
          style={{
            padding: "12px 32px",
            borderTop: "1px solid #e5e7eb",
            backgroundColor: "#ffffff",
            display: "flex",
            justifyContent: "flex-end",
            gap: "8px",
          }}
        >
          <button
            type="button"
            onClick={onReset}
            style={{
              padding: "6px 12px",
              borderRadius: "4px",
              border: "1px solid #d1d5db",
              backgroundColor: "#ffffff",
              fontSize: "13px",
              cursor: "pointer",
            }}
          >
            Reset
          </button>
          <button
            type="button"
            onClick={() => onSave?.(settings)}
            style={{
              padding: "6px 16px",
              borderRadius: "4px",
              border: "none",
              backgroundColor: "#2563eb",
              color: "#ffffff",
              fontWeight: "500",
              fontSize: "13px",
              cursor: "pointer",
            }}
          >
            Save
          </button>
        </footer>
      </main>
    </div>
  );
};
