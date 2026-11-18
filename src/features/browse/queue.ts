import { api } from '../../lib/api';
import type { JobRequest } from '../../lib/types';
import { t } from '../../ui/state/app.svelte';
import { describeError } from '../../ui/state/errors';
import { go } from '../../ui/state/router.svelte';
import { toast } from '../../ui/state/toasts.svelte';

/**
 * Queues a download and tells the user what happened: queued, already in
 * the queue (a second click does not download twice), or why it failed.
 * Returns whether anything new was queued.
 */
export async function queueJob(request: JobRequest, queuedMessage: string): Promise<boolean> {
  const showQueue = { label: t('nav.downloads'), run: () => go('downloads') };
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
