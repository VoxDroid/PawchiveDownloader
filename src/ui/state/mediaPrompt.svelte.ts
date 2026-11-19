/**
 * Asks what to do when a site's media servers do not answer before a
 * download starts: cancel, save thumbnails instead, or go on anyway.
 */

export type MediaChoice = 'cancel' | 'thumbnails' | 'continue';

export const mediaPrompt = $state({
  open: false,
  host: '',
  site: '',
});

let resolver: ((choice: MediaChoice) => void) | null = null;

export function askMediaDown(host: string, site: string): Promise<MediaChoice> {
  resolver?.('cancel');
  mediaPrompt.host = host;
  mediaPrompt.site = site;
  mediaPrompt.open = true;
  return new Promise((resolve) => (resolver = resolve));
}

export function answerMediaDown(choice: MediaChoice): void {
  mediaPrompt.open = false;
  resolver?.(choice);
  resolver = null;
}
