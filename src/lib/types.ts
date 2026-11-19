import type { SiteId } from './urls';
/**
 * Shapes shared with the Rust side (serde, camelCase). Keep in step with
 * src-tauri/src/**.
 */

export type FileCategory = 'image' | 'video' | 'audio' | 'archive' | 'document' | 'text' | 'other';
export type FileRole = 'file' | 'attachment' | 'inline';

export interface PostFile {
  index: number;
  role: FileRole;
  name: string;
  ext: string;
  hash: string | null;
  path: string;
  url: string | null;
  thumbnailUrl: string | null;
  previewOnly: boolean;
  category: FileCategory;
}

export interface Post {
  id: string;
  service: string;
  creatorId: string;
  title: string;
  contentHtml: string;
  published: string | null;
  added: string | null;
  edited: string | null;
  tags: string[];
  files: PostFile[];
  url: string;
  nextId: string | null;
  prevId: string | null;
  incomplete?: boolean;
  /** Links to outside file hosts (Mega, Google Drive...) in the text. */
  links?: string[];
  /** Archive passwords found in the text. */
  passwords?: string[];
}

/** One creator in the site-wide creator search. */
export interface CreatorHit {
  id: string;
  name: string;
  service: string;
  publicId: string | null;
  favorited: number;
  /** Epoch ms of the newest import, 0 when unknown. */
  updated: number;
  url: string;
  iconUrl: string;
}

export type CreatorSort = 'relevance' | 'favorited' | 'updated' | 'name';

export interface CreatorSearchResult {
  creators: CreatorHit[];
  total: number;
  size: number;
  fetchedAt: number;
}

export interface Creator {
  id: string;
  service: string;
  name: string;
  publicId: string | null;
  url: string;
  iconUrl: string;
  bannerUrl: string;
  updated: string | null;
  postCount: number | null;
}

export type PageRef =
  | { kind: 'post'; service: string; creatorId: string; postId: string }
  | { kind: 'creator'; service: string; creatorId: string; offset: number; query: string | null }
  | { kind: 'listing'; offset: number; query: string | null; site?: SiteId }
  | { kind: 'other' };

export interface ParsedLink {
  url: string;
  ref: PageRef;
}

export interface PostView {
  post: Post;
  creator: Creator | null;
  known: string[];
  favorite: boolean;
}

export interface CreatorView {
  creator: Creator;
  watched: boolean;
  favorite: boolean;
}

export interface PostPage {
  posts: Post[];
  downloaded: string[];
  /** Ids of posts opened before. */
  seen?: string[];
}

// ------------------------------------------------------------------ settings

export const LOCALES = ['en', 'ja', 'ko', 'zh_CN', 'es', 'pt_BR', 'ru', 'de', 'fr'] as const;
export type Locale = (typeof LOCALES)[number];
export const THEMES = ['midnight', 'graphite', 'plum', 'forest', 'ember', 'light'] as const;
export type Theme = (typeof THEMES)[number];
export const ACCENTS = ['#8b7cf6', '#5aa9ff', '#3ccf91', '#f5a524', '#ff6b8b', '#22d3ee'] as const;
export const FILE_CATEGORIES: readonly FileCategory[] = [
  'image',
  'video',
  'audio',
  'archive',
  'document',
  'text',
  'other',
];
export const WATCH_INTERVALS = [0, 60, 180, 360, 720, 1440] as const;
export const MAX_CONCURRENCY = 5;

export type DownloadMode = 'individual' | 'zip' | 'thumbnails';
export type ConflictAction = 'uniquify' | 'overwrite' | 'skip';
export type PreviewOnlyAction = 'relay' | 'thumbnail' | 'skip';
export type ContentFormat = 'none' | 'txt' | 'html' | 'both';
export type ProxyMode = 'system' | 'none' | 'custom' | 'tor';
export type NotificationLevel = 'all' | 'errors' | 'none';
export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

export type FilterScope = 'title' | 'all';

/** Which posts and files a download takes; see src-tauri/src/jobs/filter.rs. */
export interface PostFilter {
  keywords: string[];
  skipWords: string[];
  scope: FilterScope;
  fileSkipWords: string[];
  extensions: string[];
  skipExtensions: string[];
  /** Newest posts a creator scan takes; 0 is no limit. */
  maxPosts: number;
}

export const EMPTY_FILTER: Readonly<PostFilter> = Object.freeze({
  keywords: [],
  skipWords: [],
  scope: 'title',
  fileSkipWords: [],
  extensions: [],
  skipExtensions: [],
  maxPosts: 0,
} satisfies PostFilter);

export function isFilterEmpty(f: PostFilter): boolean {
  return (
    !f.keywords.length &&
    !f.skipWords.length &&
    !f.fileSkipWords.length &&
    !f.extensions.length &&
    !f.skipExtensions.length &&
    !f.maxPosts
  );
}

export interface Settings {
  language: 'auto' | Locale;
  theme: Theme | 'system';
  accent: string;
  reduceMotion: 'auto' | 'on' | 'off';
  downloadDir: string;
  downloadMode: DownloadMode;
  concurrency: number;
  requestIntervalMs: number;
  maxRetries: number;
  folderTemplate: string;
  fileTemplate: string;
  archiveTemplate: string;
  conflictAction: ConflictAction;
  skipDownloaded: boolean;
  verifyHashes: boolean;
  previewOnly: PreviewOnlyAction;
  thumbnailFallback: boolean;
  saveContent: ContentFormat;
  saveMetadata: boolean;
  saveLinks: boolean;
  saveComments: boolean;
  saveAnnouncements: boolean;
  categories: Record<FileCategory, boolean>;
  speedLimitKbps: number;
  keepAwake: boolean;
  /** Retry failed files after this many minutes (0 off), up to 3 times. */
  autoRetryMinutes: number;
  /** Blur thumbnails and previews until hovered. */
  blurPreviews: boolean;
  /** Notifications without titles or creator names. */
  privateNotifications: boolean;
  /** Offer to download links copied anywhere. */
  watchClipboard: boolean;
  /** Hold new downloads below this much free space (MB); 0 is off. */
  minFreeSpaceMb: number;
  scheduleEnabled: boolean;
  /** `HH:MM`, local time. */
  scheduleStart: string;
  scheduleEnd: string;
  fastMode: boolean;
  zipMaxSizeMb: number;
  zipVideosSeparately: boolean;
  filter: PostFilter;
  /** Words taken out of post titles and file names. */
  nameRemove: string[];
  proxy: { mode: ProxyMode; url: string };
  notifications: NotificationLevel;
  watchIntervalMinutes: number;
  recordHistory: boolean;
  logLevel: LogLevel;
  closeToTray: boolean;
  startMinimized: boolean;
  launchAtLogin: boolean;
  checkUpdates: boolean;
  confirmLargeJobs: boolean;
  onboarded: boolean;
}

export const DEFAULT_SETTINGS: Readonly<Settings> = Object.freeze({
  language: 'auto',
  theme: 'midnight',
  accent: ACCENTS[0],
  reduceMotion: 'auto',
  downloadDir: '',
  downloadMode: 'individual',
  concurrency: 3,
  requestIntervalMs: 1000,
  maxRetries: 5,
  folderTemplate: '{site}/{creator} ({service})/{published:YYYY-MM-DD} {title} [{post_id}]',
  fileTemplate: '{index:2}_{filename}',
  archiveTemplate: '{creator} - {published:YYYY-MM-DD} {title} [{post_id}]',
  conflictAction: 'uniquify',
  skipDownloaded: true,
  verifyHashes: true,
  previewOnly: 'relay',
  thumbnailFallback: false,
  saveContent: 'txt',
  saveMetadata: false,
  saveLinks: true,
  saveComments: false,
  saveAnnouncements: false,
  categories: Object.fromEntries(FILE_CATEGORIES.map((c) => [c, true])) as Record<
    FileCategory,
    boolean
  >,
  speedLimitKbps: 0,
  keepAwake: true,
  autoRetryMinutes: 30,
  blurPreviews: false,
  privateNotifications: false,
  watchClipboard: false,
  minFreeSpaceMb: 1024,
  scheduleEnabled: false,
  scheduleStart: '01:00',
  scheduleEnd: '07:00',
  fastMode: false,
  zipMaxSizeMb: 0,
  zipVideosSeparately: false,
  filter: EMPTY_FILTER,
  nameRemove: [],
  proxy: { mode: 'system', url: '' },
  notifications: 'all',
  watchIntervalMinutes: 360,
  recordHistory: true,
  logLevel: 'info',
  closeToTray: true,
  startMinimized: false,
  launchAtLogin: false,
  checkUpdates: true,
  confirmLargeJobs: true,
  onboarded: false,
} satisfies Settings);

/** Maps a system UI language (`pt-BR`, `zh-Hans-CN`, `en-US`) to a locale. */
export function resolveLocale(preference: Settings['language'], uiLanguage: string): Locale {
  if (preference !== 'auto') return preference;
  const lang = uiLanguage.replace('-', '_').toLowerCase();
  if (lang.startsWith('zh')) return 'zh_CN';
  if (lang.startsWith('pt')) return 'pt_BR';
  const base = lang.split('_')[0] ?? 'en';
  return (LOCALES as readonly string[]).includes(base) ? (base as Locale) : 'en';
}

// ---------------------------------------------------------------------- jobs

export type JobStatus =
  'resolving' | 'queued' | 'running' | 'paused' | 'done' | 'failed' | 'cancelled';
export type ItemStatus = 'pending' | 'active' | 'done' | 'failed' | 'skipped' | 'cancelled';
export type SkipReason = 'duplicate' | 'preview-only' | 'no-relay' | 'exists' | 'empty';
export type JobAction = 'pause' | 'resume' | 'cancel' | 'retry' | 'remove' | 'top' | 'up' | 'down';

export type WhenDone = 'nothing' | 'quit' | 'sleep' | 'shutdown';

/** Why new transfers do not start although downloads are queued. */
export type Hold =
  | { kind: 'schedule'; opensAt: number }
  | { kind: 'disk'; free: number; need: number; folder: string };

export interface QueueStatus {
  hold: Hold | null;
  whenDone: WhenDone;
  /** When the when-done action fires (epoch ms), during its countdown. */
  actionAt: number | null;
  awake: boolean;
  /** Media hosts that are not answering (only thumbnails work). */
  downSites: string[];
}

export interface CreatorInfo {
  service: string;
  id: string;
  name: string;
}

export interface PostRef {
  service: string;
  creatorId: string;
  postId: string;
}

export interface JobOverrides {
  mode?: DownloadMode;
  categories?: FileCategory[];
  saveContent?: ContentFormat;
  saveMetadata?: boolean;
  saveComments?: boolean;
  saveAnnouncements?: boolean;
  previewOnly?: PreviewOnlyAction;
  skipDownloaded?: boolean;
  downloadDir?: string;
  /** Replaces the filter from the settings. */
  filter?: PostFilter;
}

export type JobRequest =
  | {
      kind: 'posts';
      title?: string;
      creator?: CreatorInfo | null;
      posts: Array<{ post: Post; fileIndexes?: number[] }>;
      options?: JobOverrides;
    }
  | {
      kind: 'refs';
      title?: string;
      creator?: CreatorInfo | null;
      refs: PostRef[];
      options?: JobOverrides;
    }
  | {
      kind: 'creator';
      service: string;
      creatorId: string;
      query?: string | null;
      postIds?: string[];
      since?: string | null;
      until?: string | null;
      options?: JobOverrides;
    };

export interface JobStats {
  total: number;
  done: number;
  failed: number;
  skipped: number;
  cancelled: number;
  active: number;
  bytes: number;
  totalBytes: number;
  partial: number;
}

export interface JobSummary {
  id: string;
  title: string;
  subtitle: string;
  creator: CreatorInfo | null;
  status: JobStatus;
  mode: DownloadMode;
  createdAt: number;
  startedAt: number | null;
  finishedAt: number | null;
  postCount: number;
  stats: JobStats;
  scan: { pages: number; posts: number; matched: number } | null;
  error: string | null;
  speed: number;
  current: string[];
  folder: string;
}

export interface ItemView {
  id: string;
  kind: 'file' | 'text' | 'zip';
  label: string;
  path: string;
  target: string | null;
  status: ItemStatus;
  bytes: number;
  totalBytes: number;
  attempts: number;
  error: string | null;
  skipReason: SkipReason | null;
  fallback: boolean;
  postId: string;
  building: boolean;
}

export interface BatchResult {
  url: string;
  jobId: string | null;
  error: string | null;
  duplicate: boolean;
}

// ------------------------------------------------------------------- library

export interface HistoryEntry {
  id: string;
  title: string;
  subtitle: string;
  creator: CreatorInfo | null;
  mode: DownloadMode;
  status: JobStatus;
  createdAt: number;
  startedAt: number | null;
  finishedAt: number;
  postCount: number;
  files: { done: number; failed: number; skipped: number };
  bytes: number;
  folder: string;
  firstFile: string | null;
  refs: PostRef[];
  errors: string[];
}

export interface NewPost {
  id: string;
  title: string;
  published: string | null;
  files: number;
}

export interface WatchEntry {
  key: string;
  service: string;
  creatorId: string;
  name: string;
  addedAt: number;
  lastCheckedAt: number | null;
  seenIds: string[];
  newPosts: NewPost[];
  error: string | null;
  autoDownload: boolean;
  options?: WatchOptions;
}

/** Per-creator watchlist choices; empty fields use the settings. */
export interface WatchOptions {
  downloadDir?: string | null;
  mode?: DownloadMode | null;
  filter?: PostFilter | null;
}

export interface LibraryStats {
  downloads: number;
  files: number;
  failed: number;
  skipped: number;
  bytes: number;
  posts: number;
  creators: number;
  knownFiles: number;
  firstAt: number | null;
  lastAt: number | null;
  topCreators: Array<{
    service: string;
    id: string;
    name: string;
    files: number;
    bytes: number;
    downloads: number;
  }>;
  months: Array<{ month: string; files: number; bytes: number }>;
  categories: Partial<Record<FileCategory, number>>;
}

export interface MissingPost {
  post: string;
  hashes: string[];
  example: string;
  count: number;
}

export interface LibraryCheck {
  checked: number;
  missingFiles: number;
  posts: MissingPost[];
}

export interface Favorite {
  key: string;
  kind: 'post' | 'creator';
  service: string;
  creatorId: string;
  postId: string | null;
  title: string;
  creatorName: string | null;
  note: string;
  addedAt: number;
  lastDownloadedAt: number | null;
}

export interface Counts {
  files: number;
  posts: number;
  history: number;
  watch: number;
  favorites: number;
}

export interface LogEntry {
  id: number;
  at: number;
  level: LogLevel;
  message: string;
  detail: string | null;
}

export interface UpdateInfo {
  current: string;
  latest: string | null;
  available: boolean;
  /** Where official builds are (Ko-fi). */
  url: string;
  sourceUrl: string;
  error: string | null;
}

export interface AppInfo {
  version: string;
  platform: string;
  arch: string;
  dataDir: string;
  logFile: string | null;
  userAgent: string;
  args: string[];
  channel: 'official' | 'source';
  /** A crash report from an earlier run that has not been shown yet. */
  lastCrash: string | null;
  /** Data is kept next to the program. */
  portable?: boolean;
}

export interface TemplatePreview {
  file: string;
  archive: string;
  folderError: string | null;
  fileError: string | null;
  archiveError: string | null;
}

export interface CheckReport {
  checked: number;
  found: number;
  failed: number;
}
