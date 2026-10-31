import { getCurrentWindow } from "@tauri-apps/api/window";
import { isOverlaySearch } from "./state";

/** Label of the overlay window created in src-tauri/src/overlay.rs. */
export const OVERLAY_LABEL = "overlay";

/** True when this page should render the notch pill instead of the app. */
export function isOverlayWindow(): boolean {
  if (isOverlaySearch(window.location.search)) return true;
  if (!("__TAURI_INTERNALS__" in window)) return false;
  try {
    return getCurrentWindow().label === OVERLAY_LABEL;
  } catch {
    return false;
  }
}
