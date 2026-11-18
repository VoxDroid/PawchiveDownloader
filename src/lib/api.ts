import type {
  AppInfo,
  BatchResult,
  CheckReport,
  Counts,
  Creator,
  CreatorSearchResult,
  CreatorSort,
  CreatorView,
  Favorite,
  HistoryEntry,
  ItemView,
  JobAction,
  JobOverrides,
  JobRequest,
  JobSummary,
  LibraryCheck,
  LibraryStats,
  LogEntry,
  MissingPost,
  ParsedLink,
  PostPage,
  PostView,
  QueueStatus,
  Settings,
  TemplatePreview,
  UpdateInfo,
  WatchEntry,
  WatchOptions,
  WhenDone,
} from './types';

/**
 * The bridge to the Rust side. Inside the app every call goes through Tauri's
 * IPC; in a plain browser (development, UI tests, screenshots) a demo backend
 * with sample data answers instead.
 */

export const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export interface Backend {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  listen<T>(event: string, handler: (payload: T) => void): Promise<() => void>;
}

let backend: Promise<Backend> | null = null;

function getBackend(): Promise<Backend> {
  backend ??= inTauri
    ? (async () => {
        const [{ invoke }, { listen }] = await Promise.all([
          import('@tauri-apps/api/core'),
          import('@tauri-apps/api/event'),
        ]);
        return {
          invoke: (command, args) => invoke(command, args),
          listen: async (event, handler) => listen(event, (e) => handler(e.payload as never)),
        } satisfies Backend;
      })()
    : import('./mock').then((m) => m.createMockBackend());
  return backend;
}

/** For tests: replace the backend. */
export function setBackend(next: Backend): void {
  backend = Promise.resolve(next);
}

export async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const b = await getBackend();
  return b.invoke<T>(command, args);
}

export async function on<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  const b = await getBackend();
  return b.listen<T>(event, handler);
}

/** Error messages from the Rust side arrive as plain strings. */
export function errorText(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  return String(error);
}

export const api = {
  appInfo: () => call<AppInfo>('app_info'),
  uiReady: () => call<void>('ui_ready'),
  quit: () => call<void>('quit_app'),

  getSettings: () => call<Settings>('get_settings'),
  updateSettings: (patch: Partial<Settings>) => call<Settings>('update_settings', { patch }),
  resetSettings: () => call<Settings>('reset_settings'),
  exportSettings: (path: string) => call<void>('export_settings', { path }),
  importSettings: (path: string) => call<Settings>('import_settings', { path }),
  previewTemplates: (folder: string, file: string, archive: string) =>
    call<TemplatePreview>('preview_templates', { folder, file, archive }),
  checkProxyUrl: (url: string) => call<string | null>('check_proxy_url', { url }),

  parseLinks: (text: string) => call<ParsedLink[]>('parse_links', { text }),
  getPost: (service: string, creatorId: string, postId: string) =>
    call<PostView>('get_post', { service, creatorId, postId }),
  getCreator: (service: string, creatorId: string) =>
    call<CreatorView>('get_creator', { service, creatorId }),
  linkedAccounts: (service: string, creatorId: string) =>
    call<Creator[]>('get_linked_accounts', { service, creatorId }),
  searchCreators: (
    query: string,
    options: {
      service?: string | null;
      sort?: CreatorSort;
      limit?: number;
      refresh?: boolean;
    } = {},
  ) =>
    call<CreatorSearchResult>('search_creators', {
      query,
      service: options.service ?? null,
      sort: options.sort ?? null,
      limit: options.limit ?? null,
      refresh: options.refresh ?? null,
    }),
  getCreatorPosts: (service: string, creatorId: string, offset: number, query: string | null) =>
    call<PostPage>('get_creator_posts', { service, creatorId, offset, query }),
  getRecentPosts: (offset: number, query: string | null) =>
    call<PostPage>('get_recent_posts', { offset, query }),

  createJob: (request: JobRequest) =>
    call<{ jobId: string; duplicate: boolean }>('create_job', { request }),
  queueLinks: (text: string, options?: JobOverrides) =>
    call<BatchResult[]>('queue_links', { text, options: options ?? null }),
  listJobs: () => call<JobSummary[]>('list_jobs'),
  jobItems: (jobId: string) => call<ItemView[]>('job_items', { jobId }),
  jobAction: (jobId: string, action: JobAction) => call<void>('job_action', { jobId, action }),
  jobsActionAll: (action: 'pause' | 'resume' | 'retry') =>
    call<void>('jobs_action_all', { action }),
  queueStatus: () => call<QueueStatus>('queue_status'),
  libraryStats: () => call<LibraryStats>('library_stats'),
  checkLibrary: () => call<LibraryCheck>('check_library'),
  repairLibrary: (posts: MissingPost[], redownload: boolean) =>
    call<string | null>('repair_library', { posts, redownload }),
  setWatchOptions: (key: string, options: WatchOptions) =>
    call<void>('set_watch_options', { key, options }),
  setWhenDone: (action: WhenDone) => call<void>('set_when_done', { action }),
  cancelWhenDone: () => call<void>('cancel_when_done'),
  clearFinished: () => call<void>('clear_finished'),
  retryItem: (jobId: string, itemId: string) => call<void>('retry_item', { jobId, itemId }),

  listHistory: () => call<HistoryEntry[]>('list_history'),
  removeHistory: (ids: string[]) => call<void>('remove_history', { ids }),
  clearHistory: (alsoKnownFiles: boolean) => call<void>('clear_history', { alsoKnownFiles }),
  exportHistoryCsv: (path: string) => call<void>('export_history_csv', { path }),
  redownload: (id: string) => call<string>('redownload', { id }),
  counts: () => call<Counts>('library_counts'),
  forgetKnown: () => call<void>('forget_known'),

  listWatch: () => call<WatchEntry[]>('list_watch'),
  addWatch: (service: string, creatorId: string, name: string) =>
    call<boolean>('add_watch', { service, creatorId, name }),
  removeWatch: (keys: string[]) => call<void>('remove_watch', { keys }),
  setWatchAuto: (key: string, on: boolean) => call<void>('set_watch_auto', { key, on }),
  markWatchSeen: (key: string | null, ids?: string[]) =>
    call<void>('mark_watch_seen', { key, ids: ids ?? null }),
  checkWatch: (keys?: string[]) => call<CheckReport>('check_watch', { keys: keys ?? null }),
  queueWatchNew: (key: string, ids?: string[]) =>
    call<string | null>('queue_watch_new', { key, ids: ids ?? null }),

  listFavorites: () => call<Favorite[]>('list_favorites'),
  addFavorite: (input: {
    service: string;
    creatorId: string;
    postId?: string | null;
    title: string;
    creatorName?: string | null;
  }) => call<Favorite>('add_favorite', { input }),
  updateFavorite: (key: string, title: string, note: string) =>
    call<void>('update_favorite', { key, title, note }),
  removeFavorites: (keys: string[]) => call<void>('remove_favorites', { keys }),
  queueFavorites: (keys: string[]) => call<string[]>('queue_favorites', { keys }),
  exportFavorites: (path: string) => call<number>('export_favorites', { path }),
  importFavorites: (path: string) => call<number>('import_favorites', { path }),

  getLogs: () => call<LogEntry[]>('get_logs'),
  clearLogs: () => call<void>('clear_logs'),
  exportLogs: (path: string) => call<void>('export_logs', { path }),

  openPath: (path: string) => call<void>('open_path', { path }),
  revealPath: (path: string) => call<boolean>('reveal_path', { path }),
  pathExists: (path: string) => call<boolean>('path_exists', { path }),
  ensureDir: (path: string) => call<void>('ensure_dir', { path }),
  readTextFile: (path: string) => call<string>('read_text_file', { path }),

  checkUpdates: () => call<UpdateInfo>('check_updates'),
  ackCrash: () => call<void>('ack_crash'),
};

// --------------------------------------------------------------- native bits

/** Opens a web page in the default browser. */
export async function openUrl(url: string): Promise<void> {
  if (!inTauri) {
    window.open(url, '_blank', 'noopener');
    return;
  }
  const { openUrl: open } = await import('@tauri-apps/plugin-opener');
  await open(url);
}

export async function pickFolder(title: string, defaultPath?: string): Promise<string | null> {
  if (!inTauri) return null;
  const { open } = await import('@tauri-apps/plugin-dialog');
  const result = await open({ directory: true, multiple: false, title, defaultPath });
  return typeof result === 'string' ? result : null;
}

export async function pickFile(
  title: string,
  filters: Array<{ name: string; extensions: string[] }>,
): Promise<string | null> {
  if (!inTauri) return null;
  const { open } = await import('@tauri-apps/plugin-dialog');
  const result = await open({ multiple: false, directory: false, title, filters });
  return typeof result === 'string' ? result : null;
}

export async function pickSavePath(
  title: string,
  defaultPath: string,
  filters: Array<{ name: string; extensions: string[] }>,
): Promise<string | null> {
  if (!inTauri) return null;
  const { save } = await import('@tauri-apps/plugin-dialog');
  return (await save({ title, defaultPath, filters })) ?? null;
}

export async function copyText(text: string): Promise<void> {
  await navigator.clipboard.writeText(text);
}
