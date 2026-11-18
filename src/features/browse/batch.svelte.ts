/** The batch queue dialog, opened from Browse, the menu, drops and launches. */
class BatchState {
  open = $state(false);
  text = $state('');

  show(text: string): void {
    this.text = text;
    this.open = true;
  }

  close(): void {
    this.open = false;
  }
}

export const batch = new BatchState();

/** Links opened recently in Browse, newest first. */
const RECENT_KEY = 'pdl.recent';

export interface RecentLink {
  url: string;
  title: string;
  kind: 'post' | 'creator' | 'listing';
  at: number;
}

export function loadRecent(): RecentLink[] {
  try {
    const value = JSON.parse(localStorage.getItem(RECENT_KEY) ?? '[]') as RecentLink[];
    return Array.isArray(value) ? value.slice(0, 12) : [];
  } catch {
    return [];
  }
}

export function rememberRecent(link: RecentLink): void {
  try {
    const next = [link, ...loadRecent().filter((r) => r.url !== link.url)].slice(0, 12);
    localStorage.setItem(RECENT_KEY, JSON.stringify(next));
  } catch {
    // Storage may be unavailable; recent links are only a convenience.
  }
}

export function clearRecent(): void {
  try {
    localStorage.removeItem(RECENT_KEY);
  } catch {
    // ignore
  }
}
