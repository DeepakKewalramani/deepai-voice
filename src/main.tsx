import React from "react";
import ReactDOM from "react-dom/client";
import { previewFromSearch } from "./overlay/state";
import { isOverlayWindow } from "./overlay/view";

async function rootElement(): Promise<React.ReactElement> {
  if (isOverlayWindow()) {
    // The notch pill window: no App code or app CSS, transparent page.
    document.documentElement.classList.add("overlay-view");
    const { default: Overlay } = await import("./overlay/Overlay");
    // Dev preview only: ?state=listening&pos=left forces a look in a browser.
    const preview = import.meta.env.DEV
      ? previewFromSearch(window.location.search)
      : null;
    return (
      <Overlay
        initialState={preview?.state ?? undefined}
        initialPosition={preview?.position ?? undefined}
      />
    );
  }
  const { default: App } = await import("./App");
  return <App />;
}

void rootElement().then((element) => {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>{element}</React.StrictMode>,
  );
});
