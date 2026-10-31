export interface Provider {
  id: string;
  kind: "stt" | "llm" | "tts" | string;
  adapter: string;
  model: string;
  baseUrl?: string;
  keyRef?: string;
}

export interface Hotkeys {
  dictate: string;
}

export interface Chains {
  stt: string[];
  llm: string[];
  tts: string[];
}

export interface Style {
  id: string;
  name: string;
  systemPrompt: string;
  userTemplate: string;
}

export interface Timeouts {
  sttMs: number;
  llmMs: number;
  totalDeadlineMs: number;
}

export interface Privacy {
  cloudAcknowledged: Record<string, string>;
}

export interface Settings {
  schemaVersion: number;
  hotkeys: Hotkeys;
  providers: Provider[];
  chains: Chains;
  styles: Style[];
  activeStyle: string;
  timeouts: Timeouts;
  privacy: Privacy;
  maxRecordSeconds: number;
}

export type SettingsTab =
  "general" | "providers" | "shortcuts" | "styles" | "privacy" | "about";

export const defaultSettings: Settings = {
  schemaVersion: 1,
  hotkeys: {
    dictate: "Ctrl+Alt+Space",
  },
  providers: [
    {
      id: "whisper-local",
      kind: "stt",
      adapter: "whisper_local",
      model: "base.en",
    },
  ],
  chains: {
    stt: ["whisper-local"],
    llm: [],
    tts: [],
  },
  styles: [
    {
      id: "clean",
      name: "Clean up",
      systemPrompt:
        "Clean up the dictated text. Fix punctuation and filler words. Do not add content.",
      userTemplate: "{{transcript}}",
    },
  ],
  activeStyle: "clean",
  timeouts: {
    sttMs: 8000,
    llmMs: 2500,
    totalDeadlineMs: 10000,
  },
  privacy: {
    cloudAcknowledged: {},
  },
  maxRecordSeconds: 120,
};
