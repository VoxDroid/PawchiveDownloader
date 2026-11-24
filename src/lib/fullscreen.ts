import { inTauri } from './api';

/**
 * Full screen for the image viewers. In the app the window itself goes full
 * screen (the viewer already covers the window); in a plain browser the page
 * does.
 */
export async function isFullscreen(): Promise<boolean> {
  if (inTauri) {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    return getCurrentWindow().isFullscreen();
  }
  return !!document.fullscreenElement;
}

export async function setFullscreen(on: boolean): Promise<void> {
  if (inTauri) {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().setFullscreen(on);
    return;
  }
  if (on && !document.fullscreenElement) await document.documentElement.requestFullscreen();
  else if (!on && document.fullscreenElement) await document.exitFullscreen();
}
