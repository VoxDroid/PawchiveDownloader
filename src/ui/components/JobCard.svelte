<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { JobAction, JobSummary } from '../../lib/types';
  import { api } from '../../lib/api';
  import { describeError } from '../state/errors';
  import { fmt, t } from '../state/app.svelte';
  import { toast } from '../state/toasts.svelte';
  import { openLink } from '../state/router.svelte';
  import type { IconName } from '../icons.generated';
  import Icon from './Icon.svelte';
  import IconButton from './IconButton.svelte';
  import Progress from './Progress.svelte';

  interface Props {
    job: JobSummary;
    compact?: boolean;
    details?: Snippet;
    onopen?: () => void;
  }

  let { job, compact = false, details, onopen }: Props = $props();

  const s = $derived(job.stats);
  const finished = $derived(
    job.status === 'done' || job.status === 'failed' || job.status === 'cancelled',
  );
  const settled = $derived(s.done + s.failed + s.skipped + s.cancelled);
  // Finished items plus how far the running ones are, so the bar moves
  // during big files instead of jumping when each one completes.
  const progress = $derived(s.total ? Math.min(1, (settled + (s.partial ?? 0)) / s.total) : 0);
  // Some files saved, some not: a warning rather than a plain failure.
  const partial = $derived(
    (job.status === 'done' || job.status === 'failed') && s.failed > 0 && s.done > 0,
  );
  const tone = $derived(
    partial || job.status === 'paused'
      ? 'warning'
      : job.status === 'failed'
        ? 'danger'
        : job.status === 'done'
          ? 'success'
          : job.status === 'cancelled'
            ? 'muted'
            : 'accent',
  );
  const statusIcon = $derived<IconName>(
    (
      {
        resolving: 'search',
        queued: 'clock',
        running: 'download',
        paused: 'pause',
        done: partial ? 'warning' : 'success',
        failed: partial ? 'warning' : 'error',
        cancelled: 'close',
      } as const
    )[job.status],
  );
  const statusLabel = $derived(partial ? t('status.partial') : t(`status.${job.status}`));
  const eta = $derived(
    // Sizes of files that have not started are unknown, so only estimate
    // once everything left is already downloading.
    job.speed > 0 &&
      s.totalBytes > s.bytes &&
      job.status === 'running' &&
      settled + s.active === s.total
      ? ((s.totalBytes - s.bytes) / job.speed) * 1000
      : 0,
  );

  // Files skipped because they were downloaded before and are still there.
  const already = $derived(s.already ?? 0);
  const otherSkipped = $derived(Math.max(0, s.skipped - already));
  // Nothing new came of it: say why, and offer to fetch them anyway.
  const nothingNew = $derived(finished && already > 0 && s.done === 0 && s.failed === 0);

  async function act(action: JobAction) {
    try {
      await api.jobAction(job.id, action);
    } catch (error) {
      toast(describeError(error), 'error');
    }
  }
</script>

<article class="job {tone}" class:compact class:finished>
  <div class="head">
    <span class="status-dot" data-tip={statusLabel}>
      <Icon name={statusIcon} spin={job.status === 'resolving'} />
    </span>
    <div class="titles">
      {#if onopen}
        <button type="button" class="title truncate link" onclick={onopen}>
          {job.title || t('common.untitled')}
        </button>
      {:else}
        <span class="title truncate">{job.title || t('common.untitled')}</span>
      {/if}
      <span class="sub truncate">
        {statusLabel}
        {#if job.subtitle}<span class="dot">/</span>{job.subtitle}{/if}
        {#if job.mode === 'zip'}<span class="dot">/</span>ZIP{/if}
      </span>
    </div>
    <div class="actions">
      {#if job.browseUrl}
        <IconButton
          size="sm"
          icon="compass"
          label={t('common.openInBrowse')}
          onclick={() => openLink(job.browseUrl as string)}
        />
      {/if}
      {#if job.status === 'running' || job.status === 'queued' || job.status === 'resolving'}
        <IconButton size="sm" icon="pause" label={t('common.pause')} onclick={() => act('pause')} />
      {/if}
      {#if job.status === 'paused'}
        <IconButton
          size="sm"
          icon="play"
          label={t('common.resume')}
          onclick={() => act('resume')}
        />
      {/if}
      {#if !finished}
        <IconButton
          size="sm"
          icon="stop"
          label={t('common.cancel')}
          onclick={() => act('cancel')}
        />
      {/if}
      {#if finished && (s.failed > 0 || s.cancelled > 0 || job.status === 'failed')}
        <IconButton size="sm" icon="retry" label={t('common.retry')} onclick={() => act('retry')} />
      {/if}
      {#if finished}
        <IconButton
          size="sm"
          icon="close"
          label={t('common.remove')}
          tipSide="left"
          onclick={() => act('remove')}
        />
      {/if}
    </div>
  </div>

  {#if !finished || !compact}
    <Progress
      value={progress}
      indeterminate={(job.status === 'resolving' && s.total === 0) ||
        (job.status === 'queued' && settled === 0)}
      tone={tone === 'accent' ? 'accent' : tone}
      height={compact ? 4 : 6}
      label={job.title}
    />
  {/if}

  <div class="meta mono-num">
    {#if job.status === 'resolving' && job.scan && s.total === 0}
      <span>{t('jobs.scanning', { pages: job.scan.pages, posts: job.scan.posts })}</span>
    {:else}
      <span>{t('jobs.progress', { done: s.done, total: s.total })}</span>
      {#if job.status === 'resolving' && job.scan}<span class="scan">
          {t('jobs.stillScanning', { posts: job.scan.posts })}
        </span>{/if}
      {#if s.failed}<span class="bad">{t('jobs.failed', { count: s.failed })}</span>{/if}
      {#if already}<span class="already" data-tip={t('jobs.alreadyHint')}>
          {t('jobs.already', { count: already })}
        </span>{/if}
      {#if otherSkipped}<span>{t('jobs.skipped', { count: otherSkipped })}</span>{/if}
      {#if s.bytes > 0}<span>{fmt.bytes(s.bytes)}</span>{/if}
      {#if (job.status === 'running' || job.status === 'resolving') && job.speed > 0}<span
          class="speed"
        >
          {fmt.bytes(job.speed)}/s
        </span>{/if}
      {#if eta > 0 && !compact}<span>{t('jobs.eta', { time: fmt.duration(eta) })}</span>{/if}
    {/if}
  </div>
  {#if finished && already > 0 && !compact}
    <div class="notice" class:quiet={!nothingNew}>
      <Icon name="fileCheck" />
      <span>
        {nothingNew
          ? t('jobs.nothingNew', { count: already })
          : t('jobs.someAlready', { count: already })}
      </span>
      <button class="again" onclick={() => act('redownload')}>
        <Icon name="retry" />
        {t('jobs.downloadAgain')}
      </button>
    </div>
  {/if}
  {#if job.error && job.error !== 'nothing-found'}
    <p class="error">{describeError(job.error)}</p>
  {:else if job.error === 'nothing-found'}
    <p class="error">{t('error.nothingFound')}</p>
  {/if}
  {#if !compact && job.current.length && job.status === 'running'}
    <ul class="current">
      {#each job.current as name (name)}<li class="truncate">
          <Icon name="downloadOutline" />
          {name}
        </li>{/each}
    </ul>
  {/if}
  {#if details}{@render details()}{/if}
</article>

<style>
  .job {
    --tone: var(--accent);
    display: grid;
    gap: 10px;
    padding: 14px;
    border-radius: var(--radius-lg);
    background: var(--bg-elev);
    border: 1px solid var(--border);
    transition:
      border-color var(--dur) var(--ease),
      background-color var(--dur) var(--ease);
    animation: pdl-rise var(--dur-slow) var(--ease-out);
  }
  .job:hover {
    border-color: var(--border-strong);
  }
  .compact {
    padding: 10px 12px;
    gap: 7px;
    border-radius: var(--radius-md);
  }
  .success {
    --tone: var(--success);
  }
  .warning {
    --tone: var(--warning);
  }
  .danger {
    --tone: var(--danger);
  }
  .muted {
    --tone: var(--text-faint);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .status-dot {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    flex: none;
    border-radius: 9px;
    color: var(--tone);
    background: color-mix(in srgb, var(--tone) 14%, transparent);
  }
  .compact .status-dot {
    width: 26px;
    height: 26px;
    border-radius: 8px;
  }
  .titles {
    display: grid;
    min-width: 0;
    flex: 1;
  }
  .title {
    font-weight: 700;
    font-size: var(--fs-md);
    color: var(--text);
    text-align: left;
  }
  .link {
    border: 0;
    background: transparent;
    padding: 0;
  }
  .link:hover {
    color: var(--accent-strong);
  }
  .sub {
    font-size: var(--fs-xs);
    color: var(--text-dim);
  }
  .dot {
    margin: 0 6px;
    color: var(--text-faint);
  }
  .actions {
    display: flex;
    gap: 2px;
    flex: none;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
    font-size: var(--fs-xs);
    color: var(--text-dim);
  }
  .bad {
    color: var(--danger);
  }
  .scan {
    color: var(--text-faint);
  }
  .speed {
    color: var(--accent-strong);
  }
  .error {
    font-size: var(--fs-xs);
    color: var(--danger);
    word-break: break-word;
  }
  .current {
    list-style: none;
    padding: 0;
    display: grid;
    gap: 2px;
    font-size: var(--fs-xs);
    color: var(--text-faint);
  }
  .already {
    color: var(--success);
  }
  /* Why nothing (or little) was downloaded, with a way to fetch it anyway. */
  .notice {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px 8px 12px;
    border-radius: var(--radius-md);
    border: 1px solid color-mix(in srgb, var(--success) 35%, var(--border));
    background: color-mix(in srgb, var(--success) 8%, var(--bg-elev-2));
    font-size: var(--fs-sm);
  }
  .notice > :global(.icon) {
    flex: none;
    color: var(--success);
  }
  .notice > span {
    flex: 1;
    min-width: 0;
  }
  .notice.quiet {
    border-color: var(--border);
    background: var(--bg-elev-2);
    color: var(--text-dim);
  }
  .again {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-strong);
    background: var(--bg-elev);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-xs);
    font-weight: 600;
    transition:
      background-color var(--dur-fast) var(--ease),
      border-color var(--dur-fast) var(--ease);
  }
  .again:hover {
    background: var(--bg-elev-3);
    border-color: color-mix(in srgb, var(--accent) 50%, var(--border-strong));
  }
</style>
