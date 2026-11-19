<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../../lib/api';
  import { pageUrls, serviceLabel } from '../../lib/urls';
  import type { LibraryCheck, LibraryStats } from '../../lib/types';
  import { fmt, t } from '../../ui/state/app.svelte';
  import { go, openLink } from '../../ui/state/router.svelte';
  import { toast } from '../../ui/state/toasts.svelte';
  import { describeError } from '../../ui/state/errors';
  import Button from '../../ui/components/Button.svelte';
  import Empty from '../../ui/components/Empty.svelte';
  import Icon from '../../ui/components/Icon.svelte';
  import Spinner from '../../ui/components/Spinner.svelte';
  import PageHeader from '../../ui/layout/PageHeader.svelte';

  let stats = $state<LibraryStats | null>(null);
  let report = $state<LibraryCheck | null>(null);
  let checking = $state(false);
  let repairing = $state(false);
  let hovered = $state<number | null>(null);

  onMount(() => {
    void api.libraryStats().then((s) => (stats = s));
  });

  const peak = $derived(Math.max(1, ...(stats?.months ?? []).map((m) => m.bytes)));
  const kinds = $derived(
    Object.entries(stats?.categories ?? {})
      .filter(([, n]) => n > 0)
      .sort((a, b) => b[1] - a[1]),
  );
  const kindPeak = $derived(Math.max(1, ...kinds.map(([, n]) => n)));
  const monthLabel = (month: string) => {
    const [y, m] = month.split('-').map(Number);
    return new Date(y ?? 2026, (m ?? 1) - 1, 1).toLocaleDateString(undefined, { month: 'short' });
  };

  async function check() {
    checking = true;
    try {
      report = await api.checkLibrary();
    } catch (e) {
      toast(describeError(e), 'error');
    } finally {
      checking = false;
    }
  }

  async function repair(redownload: boolean) {
    if (!report) return;
    repairing = true;
    try {
      const jobId = await api.repairLibrary(report.posts, redownload);
      toast(
        redownload ? t('library.requeued') : t('library.forgotten'),
        'success',
        jobId ? { action: { label: t('nav.downloads'), run: () => go('downloads') } } : {},
      );
      report = null;
      stats = await api.libraryStats();
    } catch (e) {
      toast(describeError(e), 'error');
    } finally {
      repairing = false;
    }
  }

  const postUrl = (key: string) => {
    const [service = '', creator = '', post = ''] = key.split(':');
    return pageUrls.post(service, creator, post);
  };
</script>

<PageHeader title={t('nav.stats')} description={t('stats.description')} />

{#if !stats}
  <div class="loading"><Spinner /></div>
{:else}
  <div class="tiles">
    <div class="tile">
      <span class="label">{t('stats.files')}</span>
      <span class="value mono-num">{stats.files.toLocaleString()}</span>
    </div>
    <div class="tile">
      <span class="label">{t('stats.size')}</span>
      <span class="value mono-num">{fmt.bytes(stats.bytes)}</span>
    </div>
    <div class="tile">
      <span class="label">{t('stats.posts')}</span>
      <span class="value mono-num">{stats.posts.toLocaleString()}</span>
    </div>
    <div class="tile">
      <span class="label">{t('stats.creators')}</span>
      <span class="value mono-num">{stats.creators.toLocaleString()}</span>
    </div>
    <div class="tile">
      <span class="label">{t('stats.downloads')}</span>
      <span class="value mono-num">{stats.downloads.toLocaleString()}</span>
    </div>
    <div class="tile">
      <span class="label">{t('stats.since')}</span>
      <span class="value small">{stats.firstAt ? fmt.date(stats.firstAt) : '-'}</span>
    </div>
  </div>

  {#if stats.downloads === 0}
    <Empty icon="database" title={t('stats.emptyTitle')} body={t('stats.emptyBody')} />
  {:else}
    <section class="card">
      <h3>{t('stats.byMonth')}</h3>
      <div class="chart" role="img" aria-label={t('stats.byMonth')}>
        {#each stats.months as m, i (m.month)}
          <div
            class="col"
            role="presentation"
            onmouseenter={() => (hovered = i)}
            onmouseleave={() => (hovered = null)}
          >
            {#if hovered === i}
              <span class="tip mono-num">
                <strong>{monthLabel(m.month)} {m.month.slice(0, 4)}</strong>
                {fmt.bytes(m.bytes)}, {t('stats.filesCount', { count: m.files })}
              </span>
            {/if}
            <span
              class="bar"
              class:zero={m.bytes === 0}
              style:height="{Math.max(m.bytes ? 2 : 0, (m.bytes / peak) * 100)}%"
            ></span>
            <span class="axis">{monthLabel(m.month)}</span>
          </div>
        {/each}
      </div>
      <details class="table">
        <summary>{t('stats.showTable')}</summary>
        <table>
          <thead>
            <tr>
              <th>{t('stats.month')}</th><th>{t('stats.files')}</th><th>{t('stats.size')}</th>
            </tr>
          </thead>
          <tbody>
            {#each stats.months as m (m.month)}
              <tr>
                <td>{m.month}</td>
                <td class="mono-num">{m.files.toLocaleString()}</td>
                <td class="mono-num">{fmt.bytes(m.bytes)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </details>
    </section>

    <div class="two">
      <section class="card">
        <h3>{t('stats.topCreators')}</h3>
        <ol class="rank">
          {#each stats.topCreators as c (`${c.service}:${c.id}`)}
            <li>
              <button onclick={() => openLink(pageUrls.creator(c.service, c.id))}>
                <span class="name truncate">{c.name}</span>
                <span class="faint">{serviceLabel(c.service)}</span>
                <span class="mono-num right">{fmt.bytes(c.bytes)}</span>
                <span class="mono-num faint">{t('stats.filesCount', { count: c.files })}</span>
              </button>
            </li>
          {/each}
        </ol>
      </section>
      <section class="card">
        <h3>{t('stats.byKind')}</h3>
        <ul class="kinds">
          {#each kinds as [kind, n] (kind)}
            <li>
              <span class="kind">{t(`category.${kind}` as never)}</span>
              <span class="track"><span style:width="{(n / kindPeak) * 100}%"></span></span>
              <span class="mono-num">{n.toLocaleString()}</span>
            </li>
          {/each}
        </ul>
        <p class="faint note">{t('stats.knownFiles', { count: stats.knownFiles })}</p>
      </section>
    </div>
  {/if}

  <section class="card">
    <div class="check-head">
      <div>
        <h3>{t('library.checkTitle')}</h3>
        <p class="faint">{t('library.checkBody')}</p>
      </div>
      <Button icon="fileCheck" loading={checking} onclick={check}>{t('library.check')}</Button>
    </div>
    {#if report}
      {#if report.missingFiles === 0}
        <p class="ok">
          <Icon name="checkCircle" />{t('library.allThere', { count: report.checked })}
        </p>
      {:else}
        <p class="warn">
          <Icon name="warning" />{t('library.missing', {
            count: report.missingFiles,
            posts: report.posts.length,
          })}
        </p>
        <ul class="missing">
          {#each report.posts.slice(0, 50) as p (p.post)}
            <li>
              <button class="truncate" title={p.example} onclick={() => openLink(postUrl(p.post))}>
                {p.example}
              </button>
              <span class="faint mono-num">{p.count}</span>
            </li>
          {/each}
        </ul>
        <div class="actions">
          <Button
            variant="primary"
            icon="download"
            loading={repairing}
            onclick={() => repair(true)}
          >
            {t('library.redownload')}
          </Button>
          <Button variant="ghost" disabled={repairing} onclick={() => repair(false)}>
            {t('library.forget')}
          </Button>
        </div>
      {/if}
    {/if}
  </section>
{/if}

<style>
  .loading {
    display: grid;
    place-items: center;
    padding: 60px 0;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    gap: 10px;
    margin-bottom: 16px;
  }
  .tile {
    display: grid;
    gap: 4px;
    padding: 14px 16px;
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    background: var(--bg-elev);
  }
  .label {
    font-size: var(--fs-xs);
    color: var(--text-dim);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .value {
    font-size: var(--fs-xl);
    font-weight: 700;
  }
  .value.small {
    font-size: var(--fs-md);
    font-weight: 600;
  }
  .card {
    margin-bottom: 16px;
    padding: 16px 18px;
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    background: var(--bg-elev);
  }
  .card h3 {
    margin: 0 0 12px;
    font-size: var(--fs-md);
  }
  .chart {
    display: grid;
    grid-template-columns: repeat(12, 1fr);
    gap: 2px;
    height: 160px;
    padding-bottom: 20px;
    border-bottom: 1px solid var(--border);
  }
  .col {
    position: relative;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    align-items: center;
    height: 100%;
    padding: 0 4px;
    cursor: default;
  }
  .bar {
    width: 100%;
    max-width: 36px;
    border-radius: 4px 4px 0 0;
    background: var(--accent);
    transition: opacity var(--dur-fast) var(--ease);
  }
  .col:hover .bar {
    opacity: 0.8;
  }
  .axis {
    position: absolute;
    bottom: -20px;
    font-size: var(--fs-xs);
    color: var(--text-faint);
  }
  .tip {
    position: absolute;
    bottom: calc(100% + 4px);
    z-index: 2;
    display: grid;
    gap: 2px;
    padding: 6px 8px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-strong);
    background: var(--bg-elev-3);
    font-size: var(--fs-xs);
    white-space: nowrap;
    box-shadow: var(--shadow-md);
  }
  .table {
    margin-top: 12px;
    font-size: var(--fs-sm);
  }
  .table summary {
    cursor: pointer;
    color: var(--text-dim);
  }
  .table table {
    margin-top: 8px;
    border-collapse: collapse;
  }
  .table th,
  .table td {
    padding: 3px 16px 3px 0;
    text-align: left;
  }
  .two {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 16px;
  }
  .two .card {
    margin-bottom: 16px;
  }
  .rank {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 2px;
  }
  .rank button {
    width: 100%;
    display: grid;
    grid-template-columns: 1fr auto auto auto;
    gap: 10px;
    align-items: center;
    padding: 6px 8px;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text);
    text-align: left;
    font-size: var(--fs-sm);
  }
  .rank button:hover {
    background: var(--bg-hover);
  }
  .name {
    font-weight: 600;
  }
  .kinds {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 8px;
  }
  .kinds li {
    display: grid;
    grid-template-columns: 90px 1fr 70px;
    gap: 10px;
    align-items: center;
    font-size: var(--fs-sm);
  }
  .kinds .mono-num {
    text-align: right;
  }
  .track {
    height: 8px;
    border-radius: 4px;
    background: var(--bg-elev-3);
    overflow: hidden;
  }
  .track span {
    display: block;
    height: 100%;
    border-radius: 4px;
    background: var(--accent);
  }
  .note {
    margin: 12px 0 0;
    font-size: var(--fs-xs);
  }
  .check-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }
  .check-head p {
    margin: -6px 0 0;
    font-size: var(--fs-sm);
    max-width: 560px;
  }
  .ok,
  .warn {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 12px 0 0;
    font-size: var(--fs-sm);
  }
  .ok :global(.icon) {
    color: var(--success);
  }
  .warn :global(.icon) {
    color: var(--warning);
  }
  .missing {
    margin: 10px 0 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 2px;
    max-height: 220px;
    overflow: auto;
  }
  .missing li {
    display: flex;
    gap: 10px;
    align-items: center;
    font-size: var(--fs-xs);
  }
  .missing button {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--text-dim);
    text-align: left;
    font: inherit;
    padding: 2px 0;
  }
  .missing button:hover {
    color: var(--text);
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 12px;
  }
</style>
