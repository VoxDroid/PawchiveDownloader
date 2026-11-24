<script lang="ts">
  import { fade } from 'svelte/transition';
  import { api } from '../../lib/api';
  import { SITES, splitService, type SiteId } from '../../lib/urls';
  import type { CreatorHit, CreatorSearchResult, CreatorSort } from '../../lib/types';
  import { fmt, t } from '../../ui/state/app.svelte';
  import { openLink } from '../../ui/state/router.svelte';
  import { describeError } from '../../ui/state/errors';
  import Avatar from '../../ui/components/Avatar.svelte';
  import Button from '../../ui/components/Button.svelte';
  import Empty from '../../ui/components/Empty.svelte';
  import Icon from '../../ui/components/Icon.svelte';
  import IconButton from '../../ui/components/IconButton.svelte';
  import Segmented from '../../ui/components/Segmented.svelte';
  import Select from '../../ui/components/Select.svelte';
  import Spinner from '../../ui/components/Spinner.svelte';

  let { query }: { query: string } = $props();

  let site = $state<SiteId>('pawchive');
  let service = $state('');
  /** The services each site archives. */
  const SITE_SERVICES: Record<SiteId, string[]> = {
    pawchive: ['patreon', 'fanbox'],
    kemono: [
      'patreon',
      'fanbox',
      'fantia',
      'gumroad',
      'subscribestar',
      'dlsite',
      'discord',
      'afdian',
      'boosty',
    ],
    coomer: ['onlyfans', 'fansly', 'candfans'],
  };
  const NAMES: Record<string, string> = {
    patreon: 'Patreon',
    fanbox: 'Fanbox',
    fantia: 'Fantia',
    gumroad: 'Gumroad',
    subscribestar: 'SubscribeStar',
    dlsite: 'DLsite',
    discord: 'Discord',
    afdian: 'Afdian',
    boosty: 'Boosty',
    onlyfans: 'OnlyFans',
    fansly: 'Fansly',
    candfans: 'CandFans',
  };
  let sort = $state<CreatorSort>('relevance');
  let result = $state<CreatorSearchResult | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(false);
  let refreshing = $state(false);
  let run = 0;

  async function search(refresh = false) {
    const id = ++run;
    loading = true;
    error = null;
    try {
      const found = await api.searchCreators(query, {
        site,
        service: service || null,
        sort,
        limit: 120,
        refresh,
      });
      if (id === run) result = found;
    } catch (e) {
      if (id === run) error = describeError(e);
    } finally {
      if (id === run) loading = false;
    }
  }

  $effect(() => {
    // Searches again whenever the query, the service or the order changes.
    void [query, site, service, sort];
    void search();
  });

  async function refresh() {
    refreshing = true;
    await search(true);
    refreshing = false;
  }

  const hidden = $derived(result ? Math.max(0, result.total - result.creators.length) : 0);

  function open(c: CreatorHit) {
    openLink(c.url);
  }
</script>

<section class="search" in:fade={{ duration: 160 }}>
  <header class="head">
    <div class="title">
      <h2>
        {#if query.trim()}
          {t('search.resultsFor', { query: query.trim() })}
        {:else}
          {t('search.popular')}
        {/if}
      </h2>
      {#if result}
        <p class="faint mono-num">
          {t('search.count', { count: result.total })}
          {#if result.fetchedAt}
            <span class="sep">/</span>{t('search.listAge', {
              when: fmt.relative(result.fetchedAt),
              size: result.size.toLocaleString(),
            })}
          {/if}
        </p>
      {/if}
    </div>
    <div class="controls">
      <Segmented
        size="sm"
        label={t('search.site')}
        bind:value={site}
        onchange={() => (service = '')}
        options={SITES.map((s) => ({ value: s.id, label: s.name }))}
      />
      <Select
        label={t('search.service')}
        bind:value={service}
        options={[
          { value: '', label: t('search.allServices') },
          ...SITE_SERVICES[site].map((s) => ({ value: s, label: NAMES[s] ?? s })),
        ]}
      />
      <Select
        label={t('search.sort')}
        bind:value={sort}
        options={[
          { value: 'relevance', label: t('search.sortRelevance') },
          { value: 'favorited', label: t('search.sortFavorited') },
          { value: 'updated', label: t('search.sortUpdated') },
          { value: 'name', label: t('search.sortName') },
        ]}
      />
      <IconButton
        icon="sync"
        label={t('search.refresh')}
        tipSide="left"
        disabled={refreshing}
        onclick={refresh}
      />
    </div>
  </header>

  {#if error}
    <Empty icon="cloudOff" title={t('search.failed')} body={error}>
      <Button icon="retry" onclick={() => search()}>{t('common.retry')}</Button>
    </Empty>
  {:else if !result}
    <div class="loading">
      <Spinner />
      <span class="faint">{t('search.loadingList')}</span>
    </div>
  {:else if result.creators.length === 0}
    <Empty icon="account" title={t('search.none')} body={t('search.noneHint')} />
  {:else}
    <ul class="list" class:dim={loading}>
      {#each result.creators as c (`${c.service}:${c.id}`)}
        <li>
          <button class="row" onclick={() => open(c)} title={c.url}>
            <Avatar src={c.iconUrl} name={c.name} size={40} radius="10px" />
            <span class="who">
              <span class="name truncate">{c.name}</span>
              <span class="meta faint truncate">
                {NAMES[splitService(c.service).service] ?? splitService(c.service).service}
                <span class="sep">/</span><span class="mono-num">{c.id}</span>
                {#if c.publicId && c.publicId !== c.name}<span class="sep">/</span>{c.publicId}{/if}
              </span>
            </span>
            <span class="stat mono-num" data-tip={t('search.favorited')}>
              <Icon name="heartOutline" />{c.favorited.toLocaleString()}
            </span>
            <span class="stat updated faint">
              {c.updated ? fmt.relative(c.updated) : ''}
            </span>
            <Icon name="chevronRight" />
          </button>
        </li>
      {/each}
    </ul>
    {#if hidden}
      <p class="more faint">{t('search.more', { count: hidden })}</p>
    {/if}
  {/if}
</section>

<style>
  .search {
    display: grid;
    gap: 14px;
    padding-top: 8px;
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    justify-content: space-between;
    gap: 12px 20px;
  }
  .title h2 {
    margin: 0;
    font-size: var(--fs-xl);
  }
  .title p {
    margin: 4px 0 0;
    font-size: var(--fs-sm);
  }
  .sep {
    margin: 0 6px;
    opacity: 0.5;
  }
  .controls {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
  }
  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 60px 0;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 6px;
    transition: opacity var(--dur) var(--ease);
  }
  .list.dim {
    opacity: 0.6;
  }
  .row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: var(--bg-elev);
    color: var(--text);
    text-align: left;
    transition:
      background-color var(--dur-fast) var(--ease),
      border-color var(--dur-fast) var(--ease);
  }
  .row:hover {
    background: var(--bg-hover);
    border-color: var(--border-strong);
  }
  .who {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 2px;
  }
  .name {
    font-weight: 600;
  }
  .meta {
    font-size: var(--fs-sm);
  }
  .stat {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: var(--fs-sm);
    color: var(--text-dim);
  }
  .updated {
    width: 110px;
    justify-content: flex-end;
  }
  .more {
    text-align: center;
    font-size: var(--fs-sm);
  }
  @media (max-width: 640px) {
    .updated {
      display: none;
    }
  }
</style>
