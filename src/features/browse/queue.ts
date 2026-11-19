import { api } from '../../lib/api';
import type { JobRequest, Post } from '../../lib/types';
import { t } from '../../ui/state/app.svelte';
import { describeError } from '../../ui/state/errors';
import { go } from '../../ui/state/router.svelte';
import { toast } from '../../ui/state/toasts.svelte';
import { askMediaDown, type MediaChoice } from '../../ui/state/mediaPrompt.svelte';
import { siteName, siteOf } from '../../lib/urls';

/**
 * Queues a download and tells the user what happened: queued, already in
 * the queue (a second click does not download twice), or why it failed.
 * Returns whether anything new was queued.
 */
export async function queueJob(
  request: JobRequest,
  queuedMessage: string,
  /** A file URL of the download, to check its media server first. */
  sample?: string | null,
): Promise<boolean> {
  const showQueue = { label: t('nav.downloads'), run: () => go('downloads') };
  if (sample) {
    const choice = await checkMedia(sample);
    if (choice === 'cancel') return false;
    if (choice === 'thumbnails') {
      request = { ...request, options: { ...request.options, mode: 'thumbnails' } };
    }
  }
  try {
    const { duplicate } = await api.createJob(request);
    if (duplicate) {
      toast(t('toast.alreadyQueued'), 'info', { action: showQueue });
      return false;
    }
    toast(queuedMessage, 'success', { action: showQueue });
    return true;
  } catch (e) {
    toast(t('toast.queueFailed', { error: describeError(e) }), 'error');
    return false;
  }
}

/** Hosts found reachable lately, so every click does not check again. */
const reachable = new Map<string, number>();
const FRESH_MS = 5 * 60 * 1000;

/**
 * Kemono's and Coomer's media servers are often down while the sites work.
 * Before such a download starts, one tiny request tells, and if the server
 * does not answer the user picks what to do.
 */
async function checkMedia(sample: string): Promise<MediaChoice> {
  let host: string;
  try {
    host = new URL(sample).hostname;
  } catch {
    return 'continue';
  }
  const site = siteOf(host);
  if (!site || site === 'pawchive') return 'continue';
  const seen = reachable.get(host);
  if (seen && Date.now() - seen < FRESH_MS) return 'continue';
  const check = await api.checkMedia(sample).catch(() => null);
  if (!check || check.reachable) {
    reachable.set(host, Date.now());
    return 'continue';
  }
  return askMediaDown(check.host, siteName(site));
}

/** The first downloadable file of some posts, for checking its server. */
export function sampleFile(posts: ReadonlyArray<Post>): string | null {
  for (const p of posts) {
    const url = p.files.find((f) => f.url)?.url;
    if (url) return url;
  }
  return null;
}
