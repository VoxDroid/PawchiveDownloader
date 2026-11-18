import type { IconName } from '../icons.generated';

export type Route =
  | 'browse'
  | 'downloads'
  | 'history'
  | 'watchlist'
  | 'favorites'
  | 'stats'
  | 'logs'
  | 'settings'
  | 'help'
  | 'about';

export const ROUTES: Array<{ id: Route; icon: IconName; group: 'main' | 'library' | 'app' }> = [
  { id: 'browse', icon: 'compass', group: 'main' },
  { id: 'downloads', icon: 'queue', group: 'main' },
  { id: 'history', icon: 'history', group: 'library' },
  { id: 'watchlist', icon: 'eye', group: 'library' },
  { id: 'favorites', icon: 'starOutline', group: 'library' },
  { id: 'stats', icon: 'database', group: 'library' },
  { id: 'logs', icon: 'logs', group: 'app' },
  { id: 'settings', icon: 'settings', group: 'app' },
  { id: 'help', icon: 'book', group: 'app' },
  { id: 'about', icon: 'info', group: 'app' },
];

function fromHash(): Route {
  const id = location.hash.replace('#', '').split('/')[0] as Route;
  return ROUTES.some((r) => r.id === id) ? id : 'browse';
}

export const router = $state({ route: fromHash() as Route, section: '' });

export function go(route: Route, section = ''): void {
  const hash = section ? `#${route}/${section}` : `#${route}`;
  if (location.hash !== hash) history.pushState(null, '', hash);
  router.route = route;
  router.section = section;
}

if (typeof window !== 'undefined') {
  window.addEventListener('popstate', () => {
    router.route = fromHash();
    router.section = location.hash.split('/')[1] ?? '';
  });
}

/** What Browse should show; set by links, history, favorites and the watchlist. */
export const browse = $state({ input: '', nonce: 0 });

export function openLink(url: string): void {
  browse.input = url;
  browse.nonce += 1;
  go('browse');
}
