import {
  BCP47,
  formatBytes as fmtBytes,
  formatDate as fmtDate,
  formatDuration as fmtDuration,
  formatNumber as fmtNumber,
  formatRelative as fmtRelative,
  translate,
  type MessageKey,
  type Params,
} from '../../lib/i18n';
import { api, on } from '../../lib/api';
import {
  DEFAULT_SETTINGS,
  resolveLocale,
  type AppInfo,
  type JobSummary,
  type Locale,
  type LogEntry,
  type QueueStatus,
  type Settings,
  type UpdateInfo,
} from '../../lib/types';

/**
 * Shared reactive state: settings (owned by the Rust side), the active
 * locale, the live job list and a few counters pushed by the backend.
 */
export const app = $state({
  settings: { ...DEFAULT_SETTINGS } as Settings,
  locale: 'en' as Locale,
  ready: false,
  jobs: [] as JobSummary[],
  watchNew: 0,
  watchChecking: false,
  systemDark: true,
  info: null as AppInfo | null,
  update: null as UpdateInfo | null,
  queue: {
    hold: null,
    whenDone: 'nothing',
    actionAt: null,
    awake: false,
    downSites: [],
  } as QueueStatus,
});

const logListeners = new Set<(entry: LogEntry) => void>();
const linkListeners = new Set<(text: string) => void>();

function applySettings(settings: Settings): void {
  app.settings = settings;
  app.locale = resolveLocale(settings.language, navigator.language);
}

let started: Promise<void> | null = null;

export function startApp(): Promise<void> {
  started ??= (async () => {
    const [settings, info, jobs, watch, queue] = await Promise.all([
      api.getSettings(),
      api.appInfo(),
      api.listJobs(),
      api.listWatch(),
      api.queueStatus(),
    ]);
    app.queue = queue;
    applySettings(settings);
    app.info = info;
    app.jobs = jobs;
    app.watchNew = watch.reduce((sum, e) => sum + e.newPosts.length, 0);
    const media = matchMedia('(prefers-color-scheme: dark)');
    app.systemDark = media.matches;
    media.addEventListener('change', (e) => (app.systemDark = e.matches));
    await Promise.all([
      on<JobSummary[]>('jobs', (jobs) => {
        app.jobs = jobs;
      }),
      on<Settings>('settings', applySettings),
      on<QueueStatus>('queue', (q) => {
        app.queue = q;
      }),
      on<{ newCount: number; checking: boolean }>('watch', (w) => {
        app.watchNew = w.newCount;
        app.watchChecking = w.checking;
      }),
      on<LogEntry>('log', (entry) => {
        for (const listener of logListeners) listener(entry);
      }),
      on<string>('open-links', (text) => {
        for (const listener of linkListeners) listener(text);
      }),
    ]);
    app.ready = true;
    if (settings.checkUpdates) {
      // Quietly, a little after start; failures only show on the About page.
      setTimeout(() => {
        void api
          .checkUpdates()
          .then((u) => {
            app.update = u;
          })
          .catch(() => undefined);
      }, 8000);
    }
  })();
  return started;
}

export async function updateSettings(patch: Partial<Settings>): Promise<void> {
  const before = app.settings;
  applySettings({ ...app.settings, ...patch });
  try {
    applySettings(await api.updateSettings(patch));
  } catch (error) {
    // Not saved: show what is in effect, not the change that failed.
    applySettings(before);
    throw error;
  }
}

export function replaceSettings(settings: Settings): void {
  applySettings(settings);
}

/** Translation bound to the current UI language; reactive in templates. */
export function t(key: MessageKey | (string & {}), params?: Params): string {
  return translate(app.locale, key, params);
}

export const fmt = {
  bytes: (n: number) => fmtBytes(n, app.locale),
  number: (n: number) => fmtNumber(n, app.locale),
  duration: (ms: number) => fmtDuration(ms, app.locale),
  relative: (at: number) => fmtRelative(at, app.locale),
  date: (value: string | number | null, withTime = false) => fmtDate(value, app.locale, withTime),
};

// ------------------------------------------------------------------ theme

export function themeAttributes(): { theme: string; motion: string } {
  const s = app.settings;
  const theme = s.theme === 'system' ? (app.systemDark ? 'midnight' : 'light') : s.theme;
  const reduce =
    s.reduceMotion === 'on' ||
    (s.reduceMotion === 'auto' && matchMedia('(prefers-reduced-motion: reduce)').matches);
  return { theme, motion: reduce ? 'reduced' : 'full' };
}

/** Themes <html> and #app, and sets the page language. */
export function themeDocument(): void {
  $effect(() => {
    const attrs = themeAttributes();
    for (const el of [document.documentElement, document.getElementById('app')]) {
      if (!el) continue;
      el.dataset.theme = attrs.theme;
      el.dataset.motion = attrs.motion;
      el.dataset.blur = app.settings.blurPreviews ? 'on' : 'off';
      el.style.setProperty('--accent', app.settings.accent);
    }
    document.documentElement.lang = BCP47[app.locale];
  });
}

export function onLog(listener: (entry: LogEntry) => void): () => void {
  logListeners.add(listener);
  return () => logListeners.delete(listener);
}

/** Links handed over by a second launch or the command line. */
export function onOpenLinks(listener: (text: string) => void): () => void {
  linkListeners.add(listener);
  return () => linkListeners.delete(listener);
}

// ------------------------------------------------------------------ jobs

export function isFinished(job: JobSummary): boolean {
  return job.status === 'done' || job.status === 'failed' || job.status === 'cancelled';
}

export function overallProgress(jobs: readonly JobSummary[]): {
  active: number;
  ratio: number;
  speed: number;
} {
  const running = jobs.filter(
    (j) => j.status === 'running' || j.status === 'queued' || j.status === 'resolving',
  );
  let done = 0;
  let total = 0;
  let speed = 0;
  for (const job of running) {
    done += job.stats.done + job.stats.failed + job.stats.skipped + (job.stats.partial ?? 0);
    total += job.stats.total;
    speed += job.speed;
  }
  return { active: running.length, ratio: total ? done / total : 0, speed };
}
