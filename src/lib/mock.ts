import type { Backend } from './api';
import {
  DEFAULT_SETTINGS,
  type Creator,
  type Favorite,
  type Gallery,
  type MediaCreator,
  type MediaFile,
  type MediaPost,
  type HistoryEntry,
  type ItemView,
  type JobRequest,
  type JobSummary,
  type LogEntry,
  type Post,
  type PostFile,
  type BridgeStatus,
  type QueueStatus,
  type Settings,
  type WatchEntry,
  type WatchOptions,
  type WhenDone,
} from './types';

/**
 * A demo backend with sample data, used when the interface runs in a plain
 * browser (development, UI tests, screenshots). Nothing here touches the
 * network: thumbnails are drawn as SVG.
 */

type Handler = (args: Record<string, unknown>) => unknown;

const ORIGIN = 'https://pawchive.pw';
const HUES = [258, 205, 152, 32, 340, 188, 280, 12];

function art(seed: number, label = ''): string {
  const h = HUES[seed % HUES.length] as number;
  const h2 = (h + 40) % 360;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 160 160"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="hsl(${h} 60% 62%)"/><stop offset="1" stop-color="hsl(${h2} 55% 38%)"/></linearGradient></defs><rect width="160" height="160" fill="url(#g)"/><circle cx="${40 + ((seed * 37) % 80)}" cy="${50 + ((seed * 23) % 60)}" r="${26 + (seed % 4) * 8}" fill="hsl(${h} 80% 85% / .35)"/><path d="M0 ${120 - (seed % 5) * 6} Q 50 ${90 + (seed % 3) * 10} 90 ${115 - (seed % 4) * 5} T 160 ${105 + (seed % 6) * 4} V160 H0Z" fill="hsl(${h2} 45% 22% / .55)"/>${label ? `<text x="10" y="150" font-family="monospace" font-size="12" fill="white" opacity=".7">${label}</text>` : ''}</svg>`;
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

const CREATORS: Creator[] = [
  {
    id: '48151623',
    service: 'patreon',
    name: 'Aster Vale',
    publicId: 'astervale',
    url: `${ORIGIN}/patreon/user/48151623`,
    iconUrl: art(1),
    bannerUrl: art(5),
    updated: '2026-11-17T08:59:08',
    postCount: 214,
  },
  {
    id: '90210',
    service: 'fanbox',
    name: 'Moss and Marrow',
    publicId: null,
    url: `${ORIGIN}/fanbox/user/90210`,
    iconUrl: art(3),
    bannerUrl: art(6),
    updated: '2026-11-15T12:00:00',
    postCount: 88,
  },
  {
    id: '777001',
    service: 'patreon',
    name: 'Lumen Studio',
    publicId: null,
    url: `${ORIGIN}/patreon/user/777001`,
    iconUrl: art(4),
    bannerUrl: art(7),
    updated: '2026-11-12T09:00:00',
    postCount: 41,
  },
];

const TITLES = [
  'Spring Sketchbook: Forest Studies',
  'Lighthouse Keeper, Final Colors',
  'Process Video: Painting Fog',
  'Brush Pack v3 (Procreate + CSP)',
  'Monthly Wallpapers - November',
  'Character Sheet: The Cartographer',
  'Behind the Scenes: Ink to Color',
  'Night Market Concept Set',
  'Timelapse: Harbor at Dawn',
  'Life Drawing Session #42',
  'Tutorial: Soft Edges and Atmosphere',
  'Postcard Series, Part 2',
];

function file(
  postSeed: number,
  i: number,
  name: string,
  category: PostFile['category'],
  previewOnly = false,
): PostFile {
  const hash = ((postSeed * 7919 + i * 104729) >>> 0).toString(16).padStart(8, '0').repeat(8);
  const ext = name.split('.').pop() ?? '';
  return {
    index: i,
    role: i === 1 ? 'file' : 'attachment',
    name,
    ext,
    hash,
    path: `/${hash.slice(0, 2)}/${hash.slice(2, 4)}/${hash}.${ext}`,
    url: previewOnly
      ? null
      : `https://n1.pawchive.pw/data/${hash}.${ext}?f=${encodeURIComponent(name)}`,
    thumbnailUrl: category === 'image' ? art(postSeed + i) : null,
    previewOnly,
    category,
  };
}

function makePost(creator: Creator, n: number): Post {
  const seed = n + creator.id.length * 31;
  const title = TITLES[n % TITLES.length] as string;
  const day = String(28 - (n % 27)).padStart(2, '0');
  const month = String(11 - (Math.floor(n / 27) % 10)).padStart(2, '0');
  const files: PostFile[] = [];
  const images = 1 + (seed % 9);
  for (let i = 1; i <= images; i++)
    files.push(
      file(
        seed,
        i,
        `${['forest', 'harbor', 'study', 'sketch', 'color'][i % 5]}-${String(i).padStart(2, '0')}.png`,
        'image',
      ),
    );
  if (n % 3 === 0)
    files.push(file(seed, files.length + 1, 'process-timelapse.mp4', 'video', n % 6 === 0));
  if (n % 4 === 1) files.push(file(seed, files.length + 1, 'source-files.zip', 'archive'));
  if (n % 5 === 2) files.push(file(seed, files.length + 1, 'notes.pdf', 'document'));
  return {
    id: String(175580000 + n * 17 + creator.id.length),
    service: creator.service,
    creatorId: creator.id,
    title: `${title}${n >= TITLES.length ? ` (${Math.floor(n / TITLES.length) + 1})` : ''}`,
    contentHtml: `<p>Thank you all for the support this month!</p><p>Included in this post:</p><ul><li>${images} full resolution images</li><li>Layered source files for higher tiers</li></ul><p>Next week: a longer process video about painting fog and soft light.</p>`,
    published: `2026-${month}-${day}T${String(8 + (n % 12)).padStart(2, '0')}:30:00`,
    added: `2026-${month}-${day}T23:00:00`,
    edited: null,
    tags: ['illustration', 'landscape', n % 2 ? 'process' : 'wallpaper'],
    files,
    url: `${ORIGIN}/${creator.service}/user/${creator.id}/post/${175580000 + n * 17 + creator.id.length}`,
    nextId: null,
    prevId: null,
    links:
      n % 4 === 0
        ? ['https://mega.nz/folder/demo#k', 'https://drive.google.com/file/d/demo/view']
        : [],
    passwords: n % 4 === 0 ? ['forest2026'] : [],
  };
}

const POSTS = new Map<string, Post[]>(
  CREATORS.map((c) => [
    c.id,
    Array.from({ length: c.id === '48151623' ? 120 : 30 }, (_, n) => makePost(c, n)),
  ]),
);

function summary(
  partial: Partial<JobSummary> & Pick<JobSummary, 'id' | 'title' | 'status'>,
): JobSummary {
  return {
    subtitle: '',
    creator: null,
    mode: 'individual',
    createdAt: Date.now() - 60_000,
    startedAt: Date.now() - 50_000,
    finishedAt: null,
    postCount: 1,
    stats: {
      total: 0,
      done: 0,
      failed: 0,
      skipped: 0,
      cancelled: 0,
      active: 0,
      bytes: 0,
      totalBytes: 0,
      partial: 0,
    },
    scan: null,
    error: null,
    speed: 0,
    current: [],
    folder: 'C:\\Users\\you\\Downloads\\Pawchive',
    ...partial,
  };
}

/** A small Library: three creators, images and a video or two. */
function demoGallery(): Gallery {
  const now = Date.now();
  const sets: Array<[string, string, string, string, number, number]> = [
    ['patreon', '48151623', 'Aster Vale', 'Spring Sketchbook: Forest Studies', 9, 1],
    ['patreon', '48151623', 'Aster Vale', 'Monthly Wallpapers - November', 6, 0],
    ['fanbox@kemono', '2207', 'Moss & Ember', 'Character Sheets Vol. 3', 7, 1],
    ['patreon', '99120', 'Lumen Studio', 'Night Market Concept Set', 12, 0],
  ];
  const posts: MediaPost[] = sets.map(
    ([service, creatorId, creatorName, title, images, videos], p) => {
      const postId = String(175500000 + p * 731);
      const files: MediaFile[] = [];
      for (let i = 0; i < images + videos; i++) {
        const video = i >= images;
        const seed = p * 17 + i;
        files.push({
          hash: (seed.toString(16) + 'a'.repeat(64)).slice(0, 64),
          name: video
            ? `timelapse-${i - images + 1}.mp4`
            : `${String(i + 1).padStart(2, '0')}_page.png`,
          kind: video ? 'video' : 'image',
          size: video ? 48_000_000 : 1_200_000 + seed * 91_000,
          path: `/downloads/${creatorName}/${title}/${i}`,
          at: now - p * 86_400_000,
          demo: video ? undefined : { src: art(seed), thumb: art(seed) },
        });
      }
      return {
        key: `${service}:${creatorId}:${postId}`,
        service,
        site: service.includes('@kemono') ? 'kemono' : 'pawchive',
        creatorId,
        postId,
        title,
        creatorName,
        folder: `/downloads/${creatorName}/${title}`,
        at: now - p * 86_400_000,
        files,
      } as MediaPost;
    },
  );
  const creators = new Map<string, MediaCreator>();
  for (const post of posts) {
    const key = `${post.service}:${post.creatorId}`;
    const c =
      creators.get(key) ??
      ({
        key,
        service: post.service,
        site: post.site,
        creatorId: post.creatorId,
        name: post.creatorName,
        posts: 0,
        images: 0,
        videos: 0,
        bytes: 0,
        at: post.at,
      } as MediaCreator);
    c.posts += 1;
    for (const f of post.files) {
      if (f.kind === 'image') c.images += 1;
      else c.videos += 1;
      c.bytes += f.size;
    }
    creators.set(key, c);
  }
  const all = posts.flatMap((p) => p.files);
  return {
    creators: [...creators.values()],
    posts,
    images: all.filter((f) => f.kind === 'image').length,
    videos: all.filter((f) => f.kind === 'video').length,
    bytes: all.reduce((n, f) => n + f.size, 0),
    missing: 0,
  };
}

export function createMockBackend(): Backend {
  let settings: Settings = {
    ...DEFAULT_SETTINGS,
    downloadDir: 'C:\\Users\\you\\Downloads\\Pawchive',
    onboarded: true,
  };
  const listeners = new Map<string, Set<(payload: unknown) => void>>();
  const emit = (event: string, payload: unknown) =>
    listeners.get(event)?.forEach((l) => l(payload));
  const aster = CREATORS[0] as Creator;
  const moss = CREATORS[1] as Creator;

  let queue: QueueStatus = {
    hold: null,
    whenDone: 'nothing',
    actionAt: null,
    awake: false,
    downSites: [],
  };
  let bridge: BridgeStatus = {
    running: true,
    port: 49517,
    pairings: [
      {
        id: 'p1',
        name: 'Edge: Pawchive Downloader',
        createdAt: Date.now() - 86_400_000,
        lastUsedAt: Date.now() - 20_000,
      },
      {
        id: 'p2',
        name: 'Firefox: Pawchive Downloader',
        createdAt: Date.now() - 9 * 86_400_000,
        lastUsedAt: Date.now() - 3 * 3_600_000,
      },
    ],
    code: null,
    codeExpiresAt: null,
  };
  let jobs: JobSummary[] = [
    summary({
      id: 'j1',
      title: 'Spring Sketchbook: Forest Studies',
      subtitle: aster.name,
      creator: { service: aster.service, id: aster.id, name: aster.name },
      status: 'running',
      stats: {
        total: 9,
        done: 3,
        failed: 0,
        skipped: 0,
        cancelled: 0,
        active: 3,
        bytes: 38_400_000,
        totalBytes: 96_000_000,
        partial: 1.2,
      },
      speed: 4_600_000,
      current: ['forest-04.png', 'forest-05.png', 'process-timelapse.mp4'],
    }),
    summary({
      id: 'j2',
      title: moss.name,
      subtitle: '',
      creator: { service: moss.service, id: moss.id, name: moss.name },
      status: 'resolving',
      postCount: 0,
      scan: { pages: 2, posts: 100, matched: 64 },
      stats: {
        total: 64,
        done: 0,
        failed: 0,
        skipped: 0,
        cancelled: 0,
        active: 0,
        bytes: 0,
        totalBytes: 0,
        partial: 0,
      },
    }),
    summary({
      id: 'j3',
      title: 'Monthly Wallpapers - November',
      subtitle: aster.name,
      status: 'done',
      mode: 'zip',
      finishedAt: Date.now() - 3_600_000,
      stats: {
        total: 2,
        done: 2,
        failed: 0,
        skipped: 0,
        cancelled: 0,
        active: 0,
        bytes: 412_000_000,
        totalBytes: 412_000_000,
        partial: 0,
      },
    }),
    summary({
      id: 'j5',
      title: 'Ink Studies Vol. 2',
      subtitle: aster.name,
      status: 'done',
      finishedAt: Date.now() - 1_800_000,
      stats: {
        total: 12,
        done: 0,
        failed: 0,
        skipped: 12,
        already: 12,
        cancelled: 0,
        active: 0,
        bytes: 0,
        totalBytes: 0,
        partial: 0,
      },
    }),
    summary({
      id: 'j4',
      title: 'Night Market Concept Set',
      subtitle: 'Lumen Studio',
      status: 'failed',
      finishedAt: Date.now() - 7_200_000,
      stats: {
        total: 14,
        done: 12,
        failed: 2,
        skipped: 0,
        cancelled: 0,
        active: 0,
        bytes: 88_000_000,
        totalBytes: 88_000_000,
        partial: 0,
      },
    }),
  ];

  let watch: WatchEntry[] = [
    {
      key: `${aster.service}:${aster.id}`,
      service: aster.service,
      creatorId: aster.id,
      name: aster.name,
      addedAt: Date.now() - 864e6,
      lastCheckedAt: Date.now() - 1_800_000,
      seenIds: [],
      newPosts: [
        {
          id: '175580201',
          title: 'Lighthouse Keeper, Final Colors',
          published: '2026-11-17T08:59:08',
          files: 6,
        },
        {
          id: '175580218',
          title: 'Timelapse: Harbor at Dawn',
          published: '2026-11-16T10:00:00',
          files: 2,
        },
      ],
      error: null,
      autoDownload: false,
    },
    {
      key: `${moss.service}:${moss.id}`,
      service: moss.service,
      creatorId: moss.id,
      name: moss.name,
      addedAt: Date.now() - 432e6,
      lastCheckedAt: Date.now() - 1_800_000,
      seenIds: [],
      newPosts: [],
      error: null,
      autoDownload: true,
    },
  ];

  let favorites: Favorite[] = [
    {
      key: `c:${aster.service}:${aster.id}`,
      kind: 'creator',
      service: aster.service,
      creatorId: aster.id,
      postId: null,
      title: aster.name,
      creatorName: aster.name,
      note: 'Landscapes, weekly process videos',
      addedAt: Date.now() - 1e9,
      lastDownloadedAt: Date.now() - 864e5,
    },
    {
      key: 'p:patreon:777001:175580042',
      kind: 'post',
      service: 'patreon',
      creatorId: '777001',
      postId: '175580042',
      title: 'Brush Pack v3 (Procreate + CSP)',
      creatorName: 'Lumen Studio',
      note: '',
      addedAt: Date.now() - 5e8,
      lastDownloadedAt: null,
    },
  ];

  const history: HistoryEntry[] = Array.from({ length: 8 }, (_, i) => ({
    id: `h${i}`,
    title: TITLES[(i * 5) % TITLES.length] as string,
    subtitle: (CREATORS[i % 3] as Creator).name,
    creator: {
      service: (CREATORS[i % 3] as Creator).service,
      id: (CREATORS[i % 3] as Creator).id,
      name: (CREATORS[i % 3] as Creator).name,
    },
    mode: i % 3 === 0 ? 'zip' : 'individual',
    status: i === 2 ? 'failed' : 'done',
    createdAt: Date.now() - (i + 1) * 9e6,
    startedAt: Date.now() - (i + 1) * 9e6,
    finishedAt: Date.now() - (i + 1) * 9e6 + 95_000,
    postCount: 1 + (i % 4),
    files: { done: 4 + i * 3, failed: i === 2 ? 2 : 0, skipped: i % 2 },
    bytes: (40 + i * 37) * 1_000_000,
    folder: `C:\\Users\\you\\Downloads\\Pawchive\\${(CREATORS[i % 3] as Creator).name}`,
    firstFile: null,
    refs: [{ service: 'patreon', creatorId: aster.id, postId: '175580201' }],
    errors: i === 2 ? ['forest-07.png: HTTP 503', 'forest-08.png: connection dropped'] : [],
  }));

  const logs: LogEntry[] = [
    {
      id: 1,
      at: Date.now() - 120_000,
      level: 'info',
      message: 'Pawchive Downloader 1.0.0 started',
      detail: null,
    },
    {
      id: 2,
      at: Date.now() - 90_000,
      level: 'info',
      message: 'Queued "Spring Sketchbook: Forest Studies" (9 items)',
      detail: null,
    },
    {
      id: 3,
      at: Date.now() - 60_000,
      level: 'warn',
      message: 'Retrying forest-07.png (1/5): HTTP 503',
      detail: null,
    },
    {
      id: 4,
      at: Date.now() - 30_000,
      level: 'info',
      message: 'Finished "Monthly Wallpapers - November": 2 done, 0 skipped, 0 failed',
      detail: null,
    },
  ];

  /** Queued requests, to answer a second identical request like the app does. */
  const queued = new Map<string, string>();

  // Running jobs move along so the interface looks alive.
  setInterval(() => {
    let changed = false;
    jobs = jobs.map((job) => {
      if (job.status !== 'running') return job;
      changed = true;
      const s = { ...job.stats };
      s.bytes = Math.min(s.totalBytes, s.bytes + job.speed / 4);
      s.partial = (s.partial + 0.18) % 3;
      if (s.bytes >= s.totalBytes * 0.98) {
        s.done = s.total;
        s.active = 0;
        return { ...job, stats: s, status: 'done', finishedAt: Date.now(), speed: 0, current: [] };
      }
      s.done = Math.min(s.total - s.active, Math.floor((s.bytes / s.totalBytes) * s.total));
      return { ...job, stats: s, speed: job.speed * (0.9 + Math.random() * 0.2) };
    });
    if (changed) emit('jobs', jobs);
  }, 250);

  const posts = (id: string) => POSTS.get(id) ?? [];
  const findPost = (postId: string) => [...POSTS.values()].flat().find((p) => p.id === postId);

  const handlers: Record<string, Handler> = {
    app_info: () => ({
      version: '1.0.0',
      platform: 'windows',
      arch: 'x86_64',
      dataDir: 'C:\\Users\\you\\AppData\\Roaming\\io.github.voxdroid.pawchivedownloader',
      logFile: null,
      userAgent: 'PawchiveDownloader/1.0.0 (+mailto:izeno.contact@gmail.com)',
      args: [],
      channel: 'official',
      lastCrash: null,
    }),
    ui_ready: () => undefined,
    quit_app: () => undefined,
    get_settings: () => settings,
    update_settings: ({ patch }) => {
      settings = { ...settings, ...(patch as Partial<Settings>) };
      emit('settings', settings);
      return settings;
    },
    reset_settings: () =>
      (settings = { ...DEFAULT_SETTINGS, downloadDir: settings.downloadDir, onboarded: true }),
    check_proxy_url: ({ url }) =>
      /^(https?|socks5h?):\/\/[^/]+/.test(String(url)) ? null : 'invalid',
    parse_links: ({ text }) =>
      String(text)
        .split(/\s+/)
        .filter((l) => l.includes('pawchive.pw'))
        .map((url) => {
          const m = /pawchive\.pw\/([\w-]+)\/user\/([\w.-]+)(?:\/post\/([\w.-]+))?/.exec(url);
          if (m?.[3])
            return { url, ref: { kind: 'post', service: m[1], creatorId: m[2], postId: m[3] } };
          if (m)
            return {
              url,
              ref: { kind: 'creator', service: m[1], creatorId: m[2], offset: 0, query: null },
            };
          if (/pawchive\.pw\/posts/.test(url))
            return {
              url,
              ref: {
                kind: 'listing',
                offset: 0,
                query: new URL(url.startsWith('http') ? url : `https://${url}`).searchParams.get(
                  'q',
                ),
              },
            };
          return { url, ref: { kind: 'other' } };
        }),
    get_post: ({ creatorId, postId }) => {
      const post =
        findPost(String(postId)) ?? makePost(CREATORS.find((c) => c.id === creatorId) ?? aster, 0);
      return {
        post,
        creator: CREATORS.find((c) => c.id === post.creatorId) ?? null,
        known: post.files.slice(0, 2).map((f) => f.hash),
        favorite: false,
      };
    },
    get_creator: ({ creatorId }) => {
      const creator = CREATORS.find((c) => c.id === creatorId);
      if (!creator) throw 'not-found';
      return {
        creator,
        watched: watch.some((w) => w.creatorId === creatorId),
        favorite: favorites.some((f) => f.key === `c:${creator.service}:${creator.id}`),
      };
    },
    get_linked_accounts: ({ creatorId }) =>
      creatorId === CREATORS[0]?.id ? CREATORS.slice(1, 2) : [],
    search_creators: ({ query, service, sort }) => {
      const q = String(query ?? '')
        .toLowerCase()
        .replace(/\s+/g, '');
      const all = CREATORS.map((c, i) => ({
        id: c.id,
        name: c.name,
        service: c.service,
        publicId: c.publicId,
        favorited: [3729, 1204, 860, 412, 95][i] ?? 10,
        updated: Date.parse(`${c.updated ?? '2026-11-01T00:00:00'}Z`),
        url: c.url,
        iconUrl: c.iconUrl,
      }));
      const hits = all
        .filter((c) => !service || c.service === service)
        .filter((c) => !q || c.name.toLowerCase().replace(/\s+/g, '').includes(q) || c.id === q);
      if (sort === 'updated') hits.sort((a, b) => b.updated - a.updated);
      else if (sort === 'name') hits.sort((a, b) => a.name.localeCompare(b.name));
      else hits.sort((a, b) => b.favorited - a.favorited);
      return { creators: hits, total: hits.length, size: 99304, fetchedAt: Date.now() - 3_600_000 };
    },
    get_creator_posts: ({ creatorId, offset, query }) => {
      const list = posts(String(creatorId)).filter(
        (p) => !query || p.title.toLowerCase().includes(String(query).toLowerCase()),
      );
      const page = list.slice(Number(offset), Number(offset) + 50);
      return { posts: page, downloaded: page.filter((_, i) => i % 7 === 3).map((p) => p.id) };
    },
    get_recent_posts: ({ offset }) => {
      const all = CREATORS.flatMap((c) => posts(c.id).slice(0, 20));
      return { posts: all.slice(Number(offset), Number(offset) + 50), downloaded: [] };
    },
    create_job: ({ request }) => {
      const signature = JSON.stringify(request);
      const same = queued.get(signature);
      if (
        same &&
        jobs.some((j) => j.id === same && !['done', 'failed', 'cancelled'].includes(j.status))
      )
        return { jobId: same, duplicate: true };
      const r = request as JobRequest;
      const id = `j${Date.now()}`;
      queued.set(signature, id);
      const title =
        r.kind === 'posts'
          ? (r.title ?? 'Post')
          : r.kind === 'refs'
            ? (r.title ?? `${r.refs.length} posts`)
            : (CREATORS.find((c) => c.id === r.creatorId)?.name ?? r.creatorId);
      jobs = [
        ...jobs,
        summary({
          id,
          title,
          status: 'running',
          createdAt: Date.now(),
          stats: {
            total: 8,
            done: 0,
            failed: 0,
            skipped: 0,
            cancelled: 0,
            active: 3,
            bytes: 0,
            totalBytes: 60_000_000,
            partial: 0,
          },
          speed: 3_000_000,
          current: ['file-01.png'],
        }),
      ];
      emit('jobs', jobs);
      return { jobId: id, duplicate: false };
    },
    queue_links: ({ text }) =>
      String(text)
        .split(/\s+/)
        .filter((l) => l.includes('pawchive.pw'))
        .map((url, i) => ({ url, jobId: `b${i}`, error: null, duplicate: false })),
    list_jobs: () => jobs,
    job_items: ({ jobId }) => {
      const job = jobs.find((j) => j.id === jobId);
      return Array.from({ length: job?.stats.total ?? 0 }, (_, i): ItemView => ({
        id: `${i}`,
        kind: 'file',
        label: `forest-${String(i + 1).padStart(2, '0')}.png`,
        path: `Pawchive/forest-${i + 1}.png`,
        target: null,
        status:
          i < (job?.stats.done ?? 0)
            ? 'done'
            : i < (job?.stats.done ?? 0) + (job?.stats.active ?? 0)
              ? 'active'
              : i >= (job?.stats.total ?? 0) - (job?.stats.failed ?? 0)
                ? 'failed'
                : 'pending',
        bytes: 4_200_000,
        totalBytes: 9_600_000,
        attempts: 0,
        error: i >= (job?.stats.total ?? 0) - (job?.stats.failed ?? 0) ? 'HTTP 503' : null,
        skipReason: null,
        fallback: false,
        building: false,
        postId: '1',
      }));
    },
    job_action: ({ jobId, action }) => {
      jobs = jobs.flatMap((j) => {
        if (j.id !== jobId) return [j];
        if (action === 'remove') return [];
        if (action === 'pause') return [{ ...j, status: 'paused' as const, speed: 0 }];
        if (action === 'resume') return [{ ...j, status: 'running' as const }];
        if (action === 'redownload')
          return [
            {
              ...j,
              status: 'done' as const,
              stats: { ...j.stats, done: j.stats.total, skipped: 0, already: 0 },
            },
          ];
        if (action === 'cancel')
          return [{ ...j, status: 'cancelled' as const, finishedAt: Date.now() }];
        return [j];
      });
      emit('jobs', jobs);
    },
    queue_status: () => queue,
    bridge_status: () => bridge,
    // The demo's Kemono media server is down, to show the prompt.
    check_media: ({ url }) => ({
      reachable: !String(url).includes('kemono'),
      host: new URL(String(url)).host,
    }),
    bridge_pair: () => {
      bridge = { ...bridge, code: '482917', codeExpiresAt: Date.now() + 120_000 };
      return bridge;
    },
    library_media: () => demoGallery(),
    open_media: () => null,
    bridge_cancel_pairing: () => {
      bridge = { ...bridge, code: null, codeExpiresAt: null };
      return bridge;
    },
    bridge_revoke: ({ id }) => {
      bridge = { ...bridge, pairings: bridge.pairings.filter((p) => p.id !== id) };
      return bridge;
    },
    library_stats: () => ({
      downloads: 48,
      files: 2214,
      failed: 3,
      skipped: 120,
      bytes: 18_400_000_000,
      posts: 391,
      creators: 5,
      knownFiles: 2214,
      firstAt: Date.parse('2026-01-12T10:00:00Z'),
      lastAt: Date.now() - 3_600_000,
      topCreators: CREATORS.map((c, i) => ({
        service: c.service,
        id: c.id,
        name: c.name,
        files: [920, 610, 380, 210, 94][i] ?? 10,
        bytes: [8.1e9, 5.2e9, 3.0e9, 1.6e9, 0.5e9][i] ?? 1e8,
        downloads: [18, 12, 9, 6, 3][i] ?? 1,
      })),
      months: Array.from({ length: 12 }, (_, i) => {
        const d = new Date(2025, 11 + i, 1);
        const bytes = [0.2, 0.4, 0.9, 1.1, 0.6, 1.8, 2.4, 1.2, 2.9, 3.1, 2.2, 1.6][i] ?? 0;
        return {
          month: `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}`,
          files: Math.round(bytes * 120),
          bytes: bytes * 1e9,
        };
      }),
      categories: { image: 1730, video: 212, archive: 160, document: 88, audio: 24 },
    }),
    check_library: () => ({
      checked: 2214,
      missingFiles: 3,
      posts: [
        {
          post: `patreon:${CREATORS[0]?.id}:175580017`,
          hashes: ['a', 'b'],
          example: 'C:/Users/me/Downloads/Pawchive/Aster Vale (patreon)/2026-05-02 Forest/01_a.png',
          count: 2,
        },
        {
          post: `fanbox:${CREATORS[1]?.id}:175580034`,
          hashes: ['c'],
          example:
            'C:/Users/me/Downloads/Pawchive/Moss and Marrow (fanbox)/2026-04-11 Reeds/02_b.png',
          count: 1,
        },
      ],
    }),
    repair_library: ({ redownload }) => (redownload ? 'job-repair' : null),
    set_watch_options: ({ key, options }) => {
      watch = watch.map((w) => (w.key === key ? { ...w, options: options as WatchOptions } : w));
    },
    set_when_done: ({ action }) => {
      queue = { ...queue, whenDone: action as WhenDone };
      emit('queue', queue);
    },
    cancel_when_done: () => {
      queue = { ...queue, whenDone: 'nothing', actionAt: null };
      emit('queue', queue);
    },
    jobs_action_all: ({ action }) => {
      jobs = jobs.map((j) =>
        action === 'pause' && j.status === 'running'
          ? { ...j, status: 'paused' }
          : action === 'resume' && j.status === 'paused'
            ? { ...j, status: 'running' }
            : j,
      );
      emit('jobs', jobs);
    },
    clear_finished: () => {
      jobs = jobs.filter((j) => !['done', 'failed', 'cancelled'].includes(j.status));
      emit('jobs', jobs);
    },
    retry_item: () => undefined,
    list_history: () => history,
    remove_history: () => undefined,
    clear_history: () => undefined,
    redownload: () => 'r1',
    library_counts: () => ({
      files: 1342,
      posts: 187,
      history: history.length,
      watch: watch.length,
      favorites: favorites.length,
    }),
    forget_known: () => undefined,
    list_watch: () => watch,
    add_watch: ({ service, creatorId, name }) => {
      watch = [
        ...watch,
        {
          key: `${service}:${creatorId}`,
          service: String(service),
          creatorId: String(creatorId),
          name: String(name),
          addedAt: Date.now(),
          lastCheckedAt: null,
          seenIds: [],
          newPosts: [],
          error: null,
          autoDownload: false,
        },
      ];
      return true;
    },
    remove_watch: ({ keys }) => {
      watch = watch.filter((w) => !(keys as string[]).includes(w.key));
    },
    set_watch_auto: ({ key, on }) => {
      watch = watch.map((w) => (w.key === key ? { ...w, autoDownload: Boolean(on) } : w));
    },
    mark_watch_seen: ({ key }) => {
      watch = watch.map((w) => (!key || w.key === key ? { ...w, newPosts: [] } : w));
      emit('watch', {
        newCount: watch.reduce((n, w) => n + w.newPosts.length, 0),
        checking: false,
      });
    },
    check_watch: () => ({ checked: watch.length, found: 0, failed: 0 }),
    queue_watch_new: () => 'w1',
    list_favorites: () => favorites,
    add_favorite: ({ input }) => {
      const i = input as { service: string; creatorId: string; postId?: string; title: string };
      const fav: Favorite = {
        key: i.postId
          ? `p:${i.service}:${i.creatorId}:${i.postId}`
          : `c:${i.service}:${i.creatorId}`,
        kind: i.postId ? 'post' : 'creator',
        service: i.service,
        creatorId: i.creatorId,
        postId: i.postId ?? null,
        title: i.title,
        creatorName: null,
        note: '',
        addedAt: Date.now(),
        lastDownloadedAt: null,
      };
      favorites = [fav, ...favorites];
      return fav;
    },
    update_favorite: ({ key, title, note }) => {
      favorites = favorites.map((f) =>
        f.key === key ? { ...f, title: String(title), note: String(note) } : f,
      );
    },
    remove_favorites: ({ keys }) => {
      favorites = favorites.filter((f) => !(keys as string[]).includes(f.key));
    },
    queue_favorites: ({ keys }) => (keys as string[]).map((k) => `f-${k}`),
    export_favorites: () => favorites.length,
    import_favorites: () => 0,
    get_logs: () => logs,
    clear_logs: () => undefined,
    export_logs: () => undefined,
    open_path: () => undefined,
    reveal_path: () => true,
    path_exists: () => true,
    ensure_dir: () => undefined,
    read_text_file: () => '',
    write_text_file: () => undefined,
    ack_crash: () => undefined,
    check_updates: () => ({
      current: '1.0.0',
      latest: '1.0.0',
      available: false,
      url: 'https://ko-fi.com/izeno',
      sourceUrl: 'https://github.com/VoxDroid/PawchiveDownloader',
      error: null,
    }),
  };

  return {
    async invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
      const handler = handlers[command];
      if (!handler) throw new Error(`Demo backend has no "${command}"`);
      await new Promise((r) => setTimeout(r, 40));
      return handler(args) as T;
    },
    async listen<T>(event: string, handler: (payload: T) => void) {
      const set = listeners.get(event) ?? new Set();
      set.add(handler as (payload: unknown) => void);
      listeners.set(event, set);
      return () => set.delete(handler as (payload: unknown) => void);
    },
  };
}
