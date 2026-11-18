<script lang="ts">
  import { describeError } from '../../ui/state/errors';
  import { onMount } from 'svelte';
  import { flip } from 'svelte/animate';
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { api } from '../../lib/api';
  import { SUPPORT_PROMPT_AFTER } from '../../lib/links';
  import type { ItemView, JobAction, WhenDone } from '../../lib/types';
  import { saveTextFile, stamp } from '../../lib/files';
  import type { IconName } from '../../ui/icons.generated';
  import { app, fmt, overallProgress, t } from '../../ui/state/app.svelte';
  import { go } from '../../ui/state/router.svelte';
  import { toast } from '../../ui/state/toasts.svelte';
  import Button from '../../ui/components/Button.svelte';
  import Empty from '../../ui/components/Empty.svelte';
  import Icon from '../../ui/components/Icon.svelte';
  import IconButton from '../../ui/components/IconButton.svelte';
  import JobCard from '../../ui/components/JobCard.svelte';
  import Segmented from '../../ui/components/Segmented.svelte';
  import Select from '../../ui/components/Select.svelte';
  import SupportCard from '../../ui/components/SupportCard.svelte';
  import VirtualList from '../../ui/components/VirtualList.svelte';
  import PageHeader from '../../ui/layout/PageHeader.svelte';

  type Filter = 'all' | 'active' | 'finished';
  let filter = $state<Filter>('all');
  let openId = $state<string | null>(null);
  let detail = $state<ItemView[] | null>(null);
  let itemFilter = $state<'all' | 'failed'>('all');

  const isLive = (status: string) =>
    status === 'running' || status === 'queued' || status === 'resolving' || status === 'paused';
  // Live jobs in queue order (the order they run in), finished ones newest first.
  const jobs = $derived(
    app.jobs
      .map((job, order) => ({ job, order }))
      .filter(({ job: j }) =>
        filter === 'all' ? true : filter === 'active' ? isLive(j.status) : !isLive(j.status),
      )
      .sort(
        (a, b) =>
          Number(isLive(b.job.status)) - Number(isLive(a.job.status)) ||
          (isLive(a.job.status) ? a.order - b.order : b.job.createdAt - a.job.createdAt),
      )
      .map(({ job }) => job),
  );
  const totals = $derived(overallProgress(app.jobs));
  const finishedCount = $derived(app.jobs.filter((j) => !isLive(j.status)).length);
  const liveCount = $derived(app.jobs.filter((j) => isLive(j.status)).length);
  const anyRunning = $derived(
    app.jobs.some(
      (j) => j.status === 'running' || j.status === 'queued' || j.status === 'resolving',
    ),
  );
  const anyPaused = $derived(app.jobs.some((j) => j.status === 'paused'));
  const failedCount = $derived(app.jobs.reduce((n, j) => n + j.stats.failed, 0));
  const items = $derived(
    (detail ?? []).filter((i) => itemFilter === 'all' || i.status === 'failed'),
  );

  // Refresh the open job's items while it runs.
  $effect(() => {
    const id = openId;
    if (!id) return;
    const load = () =>
      api
        .jobItems(id)
        .then((list) => {
          if (openId === id) detail = list;
        })
        .catch(() => undefined);
    void load();
    const timer = setInterval(() => {
      const job = app.jobs.find((j) => j.id === id);
      if (job && isLive(job.status)) void load();
    }, 700);
    return () => clearInterval(timer);
  });

  // One last refresh when the open job finishes.
  $effect(() => {
    const job = app.jobs.find((j) => j.id === openId);
    if (job && !isLive(job.status)) {
      const id = job.id;
      void api.jobItems(id).then((list) => {
        if (openId === id) detail = list;
      });
    }
  });

  function toggle(id: string) {
    if (openId === id) {
      openId = null;
      detail = null;
    } else {
      openId = id;
      detail = null;
      itemFilter = 'all';
    }
  }

  async function all(action: 'pause' | 'resume' | 'retry') {
    await api.jobsActionAll(action);
  }

  /** Writes every failed item (post link, file, error) to a text file. */
  async function exportFailed() {
    const lines: string[] = [];
    for (const job of app.jobs.filter((j) => j.stats.failed > 0)) {
      const items = await api.jobItems(job.id).catch(() => []);
      for (const item of items.filter((i) => i.status === 'failed')) {
        const post = job.creator
          ? `https://pawchive.pw/${job.creator.service}/user/${job.creator.id}/post/${item.postId}`
          : item.postId;
        lines.push([post, item.label, item.error ?? ''].join('\t'));
      }
    }
    if (!lines.length) return;
    const path = await saveTextFile(
      `pawchive-failed-${stamp()}.txt`,
      `${lines.join('\n')}\n`,
      t('downloads.exportFailed'),
    );
    if (path) toast(t('downloads.exportedFailed', { count: lines.length }), 'success');
  }

  function whenDone(action: WhenDone) {
    void api.setWhenDone(action);
  }

  async function act(jobId: string, action: JobAction) {
    await api.jobAction(jobId, action).catch((e) => toast(describeError(e), 'error'));
  }

  async function showFile(path: string | null) {
    if (!path) return;
    const found = await api.revealPath(path).catch(() => false);
    if (!found) toast(t('downloads.fileMissing'), 'warning');
  }

  async function openFolder(path: string) {
    await api.ensureDir(path).catch(() => undefined);
    await api.openPath(path).catch(() => toast(t('history.folderMissing'), 'warning'));
  }

  // A one-time thank-you once someone has finished a few downloads.
  let showSupport = $state(false);
  onMount(async () => {
    let dismissed: boolean;
    try {
      dismissed = localStorage.getItem('pdl.supportDismissed') === '1';
    } catch {
      dismissed = false;
    }
    const counts = await api.counts().catch(() => ({ history: 0 }));
    showSupport = !dismissed && counts.history >= SUPPORT_PROMPT_AFTER;
  });
  function dismissSupport() {
    showSupport = false;
    try {
      localStorage.setItem('pdl.supportDismissed', '1');
    } catch {
      // ignore
    }
  }

  const ITEM_ICON: Record<ItemView['status'], IconName> = {
    pending: 'clock',
    active: 'download',
    done: 'success',
    failed: 'error',
    skipped: 'skip',
    cancelled: 'close',
  };
</script>

<PageHeader title={t('nav.downloads')} description={t('downloads.description')}>
  {#snippet actions()}
    {#if anyRunning}<Button icon="pause" onclick={() => all('pause')}>
        {t('downloads.pauseAll')}
      </Button>{/if}
    {#if anyPaused}<Button icon="play" onclick={() => all('resume')}>
        {t('downloads.resumeAll')}
      </Button>{/if}
    {#if failedCount}<Button icon="retry" onclick={() => all('retry')}>
        {t('downloads.retryFailed', { count: failedCount })}
      </Button>
      <Button variant="ghost" icon="export" onclick={exportFailed}>
        {t('downloads.exportFailed')}
      </Button>{/if}
    <Button
      variant="ghost"
      icon="trash"
      disabled={finishedCount === 0}
      onclick={() => api.clearFinished()}
    >
      {t('downloads.clearFinished')}
    </Button>
  {/snippet}
</PageHeader>

{#if showSupport}
  <div class="support"><SupportCard ondismiss={dismissSupport} /></div>
{/if}

<div class="stats">
  <div class="stat">
    <span class="label">{t('downloads.statActive')}</span>
    <span class="value mono-num">{totals.active}</span>
  </div>
  <div class="stat">
    <span class="label">{t('downloads.statSpeed')}</span>
    <span class="value mono-num">{totals.speed > 0 ? `${fmt.bytes(totals.speed)}/s` : '-'}</span>
  </div>
  <div class="stat">
    <span class="label">{t('downloads.statProgress')}</span>
    <span class="value mono-num">
      {totals.active > 0 ? `${Math.round(totals.ratio * 100)}%` : '-'}
    </span>
  </div>
  <div class="stat">
    <span class="label">{t('downloads.statFinished')}</span>
    <span class="value mono-num">{finishedCount}</span>
  </div>
</div>

<div class="toolbar">
  <Segmented
    label={t('downloads.filter')}
    size="sm"
    bind:value={filter}
    options={[
      { value: 'all', label: t('downloads.filterAll') },
      { value: 'active', label: t('downloads.filterActive') },
      { value: 'finished', label: t('downloads.filterFinished') },
    ]}
  />
  <label class="when-done">
    <Icon name="power" />
    <span class="faint">{t('whenDone.label')}</span>
    <Select
      label={t('whenDone.label')}
      value={app.queue.whenDone}
      onchange={whenDone}
      options={[
        { value: 'nothing', label: t('whenDone.nothing') },
        { value: 'quit', label: t('whenDone.quit') },
        { value: 'sleep', label: t('whenDone.sleep') },
        { value: 'shutdown', label: t('whenDone.shutdown') },
      ]}
    />
  </label>
</div>

{#if jobs.length === 0}
  <Empty icon="queue" title={t('downloads.emptyTitle')} body={t('downloads.emptyBody')}>
    <Button variant="primary" icon="compass" onclick={() => go('browse')}>
      {t('downloads.browse')}
    </Button>
  </Empty>
{:else}
  <div class="list">
    {#each jobs as job (job.id)}
      <div animate:flip={{ duration: 260, easing: cubicOut }}>
        <JobCard {job} onopen={() => toggle(job.id)}>
          {#snippet details()}
            <div class="details-toggle">
              <Button
                size="sm"
                variant="ghost"
                iconRight={openId === job.id ? 'chevronUp' : 'chevronDown'}
                onclick={() => toggle(job.id)}
              >
                {openId === job.id
                  ? t('downloads.hideItems')
                  : t('downloads.showItems', { count: job.stats.total })}
              </Button>
              <span class="job-tools">
                {#if isLive(job.status) && liveCount > 1}
                  <IconButton
                    size="sm"
                    icon="toTop"
                    label={t('downloads.toTop')}
                    onclick={() => act(job.id, 'top')}
                  />
                  <IconButton
                    size="sm"
                    icon="arrowUp"
                    label={t('downloads.moveUp')}
                    onclick={() => act(job.id, 'up')}
                  />
                  <IconButton
                    size="sm"
                    icon="arrowDown"
                    label={t('downloads.moveDown')}
                    onclick={() => act(job.id, 'down')}
                  />
                {/if}
                <IconButton
                  size="sm"
                  icon="folderOpen"
                  label={t('downloads.openFolder')}
                  tipSide="left"
                  onclick={() => openFolder(job.folder)}
                />
                {#if job.finishedAt}<span class="faint">{fmt.relative(job.finishedAt)}</span>{/if}
              </span>
            </div>
            {#if openId === job.id}
              <div class="details" transition:slide={{ duration: 220 }}>
                {#if !detail}
                  <p class="muted pad">{t('common.loading')}</p>
                {:else}
                  <div class="item-tools">
                    <Segmented
                      label={t('downloads.filter')}
                      size="sm"
                      bind:value={itemFilter}
                      options={[
                        { value: 'all', label: t('downloads.itemsAll') },
                        { value: 'failed', label: t('downloads.itemsFailed') },
                      ]}
                    />
                  </div>
                  <div
                    class="items"
                    style="height:{Math.min(360, Math.max(1, items.length) * 40)}px"
                  >
                    <VirtualList {items} rowHeight={40} key={(i) => i.id}>
                      {#snippet row(item)}
                        <div class="item {item.status}">
                          <Icon name={ITEM_ICON[item.status]} />
                          <div class="item-main">
                            <span class="truncate" title={item.path}>{item.label}</span>
                            {#if item.error || item.skipReason}
                              <span class="item-note truncate" title={item.error}>
                                {item.skipReason
                                  ? t(`skip.${item.skipReason}`)
                                  : describeError(item.error)}
                              </span>
                            {/if}
                          </div>
                          {#if item.status === 'active' && item.totalBytes > 0}
                            <span
                              class="item-bar"
                              style="--p:{Math.min(1, item.bytes / item.totalBytes)}"
                            >
                              <span></span>
                            </span>
                          {/if}
                          <span class="mono-num faint">
                            {item.bytes > 0
                              ? item.status === 'active' && item.totalBytes > 0
                                ? `${fmt.bytes(item.bytes)} / ${fmt.bytes(item.totalBytes)}`
                                : fmt.bytes(item.bytes)
                              : ''}
                          </span>
                          {#if item.building}<span class="tag building">
                              {t('downloads.building')}
                            </span>{/if}
                          {#if item.fallback}<span
                              class="tag"
                              data-tip={t('downloads.fallbackHint')}
                            >
                              {t('file.preview')}
                            </span>{/if}
                          {#if item.status === 'done'}
                            <IconButton
                              size="sm"
                              icon="folderOpen"
                              label={t('downloads.showInFolder')}
                              tipSide="left"
                              onclick={() => showFile(item.target)}
                            />
                          {:else if item.status === 'failed' || item.status === 'cancelled'}
                            <IconButton
                              size="sm"
                              icon="retry"
                              label={t('common.retry')}
                              tipSide="left"
                              onclick={() => api.retryItem(job.id, item.id)}
                            />
                          {/if}
                        </div>
                      {/snippet}
                      {#snippet empty()}
                        <p class="muted pad">{t('downloads.noItems')}</p>
                      {/snippet}
                    </VirtualList>
                  </div>
                {/if}
              </div>
            {/if}
          {/snippet}
        </JobCard>
      </div>
    {/each}
  </div>
{/if}

<style>
  .when-done {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    margin-left: auto;
    font-size: var(--fs-sm);
  }
  .support {
    margin-bottom: 20px;
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 12px;
    margin-bottom: 20px;
  }
  .stat {
    display: grid;
    gap: 4px;
    padding: 14px 16px;
    border-radius: var(--radius-lg);
    background: var(--bg-elev);
    border: 1px solid var(--border);
  }
  .label {
    font-size: var(--fs-xs);
    color: var(--text-faint);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-weight: 600;
  }
  .value {
    font-size: var(--fs-xl);
    font-weight: 700;
  }
  .toolbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px 16px;
    margin-bottom: 14px;
  }
  .toolbar :global(.segmented) {
    width: 320px;
  }
  .list {
    display: grid;
    gap: 10px;
  }
  .details-toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: -4px -6px -6px;
    font-size: var(--fs-xs);
  }
  .details {
    display: grid;
    gap: 10px;
    border-top: 1px solid var(--border);
    padding-top: 10px;
  }
  .item-tools :global(.segmented) {
    width: 220px;
  }
  .items {
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    overflow: hidden;
    background: var(--bg);
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 100%;
    padding: 0 8px 0 12px;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
    font-size: var(--fs-sm);
  }
  .item > :global(.icon) {
    color: var(--text-faint);
  }
  .done > :global(.icon) {
    color: var(--success);
  }
  .failed > :global(.icon) {
    color: var(--danger);
  }
  .active > :global(.icon) {
    color: var(--accent-strong);
  }
  .item-main {
    flex: 1;
    min-width: 0;
    display: grid;
    line-height: 1.3;
  }
  .item-note {
    font-size: var(--fs-xs);
    color: var(--text-faint);
  }
  .failed .item-note {
    color: var(--danger);
  }
  .pad {
    padding: 12px;
  }
  .job-tools {
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }
  .job-tools .faint {
    margin-left: 8px;
  }
  .item-bar {
    width: 90px;
    height: 4px;
    border-radius: 99px;
    background: var(--bg-elev-3);
    overflow: hidden;
    flex: none;
  }
  .item-bar span {
    display: block;
    height: 100%;
    background: var(--accent);
    transform-origin: left;
    transform: scaleX(var(--p));
    transition: transform 600ms var(--ease-out);
  }
  .tag.building {
    color: var(--accent-strong);
    background: var(--accent-soft);
    animation: pdl-pulse 1.4s var(--ease) infinite;
  }
  .tag {
    font-size: var(--fs-xs);
    padding: 1px 7px;
    border-radius: 99px;
    color: var(--warning);
    background: color-mix(in srgb, var(--warning) 14%, transparent);
  }
</style>
