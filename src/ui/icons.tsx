import React from "react";

/** Small line icons drawn inline so the app ships no image assets. */
const Svg: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <svg
    width="18"
    height="18"
    viewBox="0 0 18 18"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.6"
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
    focusable="false"
  >
    {children}
  </svg>
);

export const HomeIcon = () => (
  <Svg>
    <path d="M3 8l6-5 6 5v6.5a1 1 0 0 1-1 1h-3v-4H7v4H4a1 1 0 0 1-1-1z" />
  </Svg>
);

export const BookIcon = () => (
  <Svg>
    <path d="M4 3.5h8.5a1.5 1.5 0 0 1 1.5 1.5v10H5.5A1.5 1.5 0 0 1 4 13.5z" />
    <path d="M4 13.5A1.5 1.5 0 0 1 5.5 12H14M7 6.5h4" />
  </Svg>
);

export const StylesIcon = () => (
  <Svg>
    <path d="M3 5h12M3 9h8M3 13h10" />
  </Svg>
);

export const GearIcon = () => (
  <Svg>
    <circle cx="9" cy="9" r="2.3" />
    <path d="M9 2v2M9 14v2M2 9h2M14 9h2M4 4l1.4 1.4M12.6 12.6L14 14M4 14l1.4-1.4M12.6 5.4L14 4" />
  </Svg>
);

export const LockIcon = () => (
  <Svg>
    <rect x="3.5" y="8" width="11" height="7.5" rx="2" />
    <path d="M6 8V6a3 3 0 0 1 6 0v2" />
  </Svg>
);

/** Waveform brand mark. */
export const LogoMark = () => (
  <span className="brand-mark" aria-hidden="true">
    <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
      <g stroke="currentColor" strokeWidth="2" strokeLinecap="round">
        <path d="M3 7.5v3" />
        <path d="M6.5 4.5v9" />
        <path d="M10 2.5v13" />
        <path d="M13.5 6v6" />
        <path d="M16 8v2" />
      </g>
    </svg>
  </span>
);
