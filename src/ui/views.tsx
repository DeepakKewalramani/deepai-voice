import React from "react";
import { StatusCard } from "./StatusCard";
import { DictationStatus } from "./status";
import { LockIcon } from "./icons";
import { DictionaryPage } from "../dictionary/DictionaryPage";
import { UseDictionary, useDictionary } from "../dictionary/useDictionary";

export const ViewHeader: React.FC<{ title: string; subtitle: string }> = ({
  title,
  subtitle,
}) => (
  <header className="view-header">
    <h2 className="view-title">{title}</h2>
    <p className="view-subtitle">{subtitle}</p>
  </header>
);

const steps: { title: string; body: React.ReactNode }[] = [
  {
    title: "Hold Fn",
    body: "In any app, put your cursor where the text should go.",
  },
  { title: "Speak", body: "Talk naturally, the way you would to a person." },
  { title: "Release", body: "Your words are pasted where your cursor is." },
];

export const HomeView: React.FC<{
  status: DictationStatus;
  partial?: string;
  onFixPermissions?: () => void;
}> = ({ status, partial = "", onFixPermissions }) => (
  <div className="view">
    <ViewHeader title="Home" subtitle="Dictate into any app on your Mac." />
    <StatusCard
      status={status}
      partial={partial}
      onFixPermissions={onFixPermissions}
    />
    <section className="how" aria-labelledby="how-title">
      <h3 id="how-title" className="section-title">
        How it works
      </h3>
      <ol className="how-steps">
        {steps.map((s, i) => (
          <li key={s.title} className="how-step">
            <span className="how-num" aria-hidden="true">
              {i + 1}
            </span>
            <strong className="how-step-title">{s.title}</strong>
            <span className="how-step-body">{s.body}</span>
          </li>
        ))}
      </ol>
    </section>
    <section className="note-card" aria-labelledby="privacy-note-title">
      <span className="note-icon">
        <LockIcon />
      </span>
      <div className="note-body">
        <h3 id="privacy-note-title" className="note-title">
          Your dictations are not stored
        </h3>
        <p className="note-text">
          Audio and text are handled in memory only. Nothing you say is saved to
          disk or written to logs.
        </p>
      </div>
    </section>
  </div>
);

export const DictionaryView: React.FC<{ dict?: UseDictionary }> = ({
  dict,
}) => (
  <div className="view dict">
    <ViewHeader
      title="Dictionary"
      subtitle="Words and names you want spelled your way."
    />
    {dict ? <DictionaryPage dict={dict} /> : <LiveDictionary />}
  </div>
);

const LiveDictionary: React.FC = () => (
  <DictionaryPage dict={useDictionary()} />
);
