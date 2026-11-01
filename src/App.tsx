import React, { useState } from "react";
import "./ui/theme.css";
import "./ui/shell.css";
import { SettingsShell, SettingsTab } from "./settings";
import { missingRequired } from "./settings/permissions";
import { usePermissions } from "./settings/usePermissions";
import { useDictationPartial } from "./ui/useDictationPartial";
import { useDictationStatus } from "./ui/useDictationStatus";
import { DictionaryView, HomeView, ViewHeader } from "./ui/views";
import {
  BookIcon,
  GearIcon,
  HomeIcon,
  LogoMark,
  PlugIcon,
  StylesIcon,
} from "./ui/icons";

type View = "home" | "dictionary" | "providers" | "styles" | "settings";

const nav: { id: View; label: string; icon: React.ReactNode }[] = [
  { id: "home", label: "Home", icon: <HomeIcon /> },
  { id: "dictionary", label: "Dictionary", icon: <BookIcon /> },
  { id: "providers", label: "Providers", icon: <PlugIcon /> },
  { id: "styles", label: "Styles", icon: <StylesIcon /> },
  { id: "settings", label: "Settings", icon: <GearIcon /> },
];

function App({ initialView = "home" }: { initialView?: View }) {
  const status = useDictationStatus();
  const partial = useDictationPartial();
  const [view, setView] = useState<View>(initialView);
  const [settingsTab, setSettingsTab] = useState<SettingsTab | undefined>();
  const permissions = usePermissions({ poll: false });
  const fixPermissions = missingRequired(permissions.status)
    ? () => {
        setSettingsTab("permissions");
        setView("settings");
      }
    : undefined;

  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="brand">
          <LogoMark />
          <h1>DeepAI Voice</h1>
        </div>
        <nav className="nav" aria-label="Main">
          <ul>
            {nav.map((item) => (
              <li key={item.id}>
                <button
                  type="button"
                  className="nav-item"
                  aria-current={view === item.id ? "page" : undefined}
                  onClick={() => {
                    setSettingsTab(undefined);
                    setView(item.id);
                  }}
                  title={item.label}
                >
                  {item.icon}
                  <span className="nav-label">{item.label}</span>
                </button>
              </li>
            ))}
          </ul>
        </nav>
        <p className="sidebar-footer">Version 0.1.0</p>
      </aside>
      <main className="content">
        {view === "home" && (
          <HomeView
            status={status}
            partial={partial}
            onFixPermissions={fixPermissions}
          />
        )}
        {view === "dictionary" && <DictionaryView />}
        {view === "providers" && (
          <div className="view view-fill">
            <ViewHeader
              title="Providers"
              subtitle="Connect speech, language and voice providers, or add your own."
            />
            <SettingsShell
              activeTab="providers"
              tabs={["providers"]}
              showFooter={false}
            />
          </div>
        )}
        {view === "styles" && (
          <div className="view view-fill">
            <ViewHeader
              title="Styles"
              subtitle="Choose how your dictated text should be cleaned up."
            />
            <SettingsShell activeTab="styles" tabs={["styles"]} />
          </div>
        )}
        {view === "settings" && (
          <div className="view view-fill">
            <ViewHeader
              title="Settings"
              subtitle="Recording, permissions, shortcuts and privacy."
            />
            <SettingsShell
              activeTab={settingsTab}
              tabs={["general", "permissions", "shortcuts", "privacy", "about"]}
            />
          </div>
        )}
      </main>
    </div>
  );
}

export default App;
