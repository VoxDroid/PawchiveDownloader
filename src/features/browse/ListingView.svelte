<script lang="ts">
  import { describeError } from '../../ui/state/errors';
  import { untrack } from 'svelte';
  import { fade } from 'svelte/transition';
  import { api } from '../../lib/api';
  import { SITES, type SiteId } from '../../lib/urls';
  import Segmented from '../../ui/components/Segmented.svelte';
  import type { JobOverrides, Post } from '../../lib/types';
  import { t } from '../../ui/state/app.svelte';
  import Button from '../../ui/components/Button.svelte';
  import Empty from '../../ui/components/Empty.svelte';
  import IconButton from '../../ui/components/IconButton.svelte';
  import Spinner from '../../ui/components/Spinner.svelte';
  import TextInput from '../../ui/components/TextInput.svelte';
  import DownloadBar from './DownloadBar.svelte';
  import { queueJob } from './queue';
  import PostList from './PostList.svelte';

  let {
    initialQuery,
    initialSite = 'pawchive',
  }: { initialQuery: string | null; initialSite?: SiteId } = $props();

  let site = $state<SiteId>('pawchive');
  const PAGE = 50;

  let posts = $state<Post[]>([]);
  let downloaded = $state(new Set<string>());
  let opened = $state(new Set<string>());
  let selected = $state(new Set<string>());
  let offset = $state(0);
  let end = $state(false);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let query = $state('');
  let applied = $state<string | null>(null);
  let busy = $state(false);

  async function loadMore() {
    if (loading || end) return;
    loading = true;
    error = null;
    try {
      const page = await api.getRecentPosts(offset, applied, site);
      const seen = new Set(posts.map((p) => p.id));
      posts = [...posts, ...page.posts.filter((p) => !seen.has(p.id))];
      downloaded = new Set([...downloaded, ...page.downloaded]);
      opened = new Set([...opened, ...(page.seen ?? [])]);
      offset += PAGE;
      if (page.posts.length < PAGE) end = true;
    } catch (e) {
      error = describeError(e);
    } finally {
      loading = false;
    }
  }

  function reset(q: string | null) {
    applied = q;
    query = q ?? '';
    posts = [];
    offset = 0;
    end = false;
    selected = new Set();
    void loadMore();
  }

  // Only a new query starts over. `reset` reads the loading state, and
  // tracking that here restarted the list after every page, forever.
  $effect(() => {
    const q = initialQuery;
    const s = initialSite;
    untrack(() => {
      site = s;
      reset(q);
    });
  });

  async function download(options: JobOverrides) {
    busy = true;
    const refs = posts
      .filter((p) => selected.has(p.id))
      .map((p) => ({ service: p.service, creatorId: p.creatorId, postId: p.id }));
    const queued = await queueJob(
      { kind: 'refs', title: t('selection.jobTitle', { count: refs.length }), refs, options },
      t('toast.queuedPosts', { count: refs.length }),
    );
    if (queued) selected = new Set();
    busy = false;
  }
</script>

<section class="listing" in:fade={{ duration: 200 }}>
  <header>
    <div>
      <h2>{applied ? t('listing.searchTitle', { query: applied }) : t('listing.recentTitle')}</h2>
      <p class="faint">{t('listing.body')}</p>
    </div>
    <Segmented
      size="sm"
      label={t('search.site')}
      bind:value={site}
      onchange={() => reset(applied)}
      options={SITES.map((s) => ({ value: s.id, label: s.name }))}
    />
    <form
      onsubmit={(e) => {
        e.preventDefault();
        reset(query.trim() || null);
      }}
    >
      <TextInput
        label={t('listing.search')}
        placeholder={t('listing.search')}
        icon="search"
        bind:value={query}
        clearable
      />
      <Button size="sm" type="submit">{t('common.search')}</Button>
    </form>
  </header>

  {#if error && !posts.length}
    <Empty icon="cloudOff" title={t('listing.loadFailed')} body={error}>
      <Button icon="retry" onclick={loadMore}>{t('common.retry')}</Button>
    </Empty>
  {:else if loading && !posts.length}
    <div class="loading"><Spinner size={22} /><span>{t('common.loading')}</span></div>
  {:else}
    <div class="bar">
      <span class="faint mono-num">
        {t('creator.loaded', { shown: posts.length, total: posts.length })}
      </span>
      <span>
        <IconButton
          size="sm"
          icon="selectAll"
          label={t('common.selectAll')}
          onclick={() => (selected = new Set(posts.map((p) => p.id)))}
        />
        <IconButton
          size="sm"
          icon="selectNone"
          label={t('common.selectNone')}
          tipSide="left"
          onclick={() => (selected = new Set())}
        />
      </span>
    </div>
    <PostList
      {posts}
      {selected}
      {downloaded}
      seen={opened}
      showCreator
      onchange={(next) => (selected = next)}
    />
    {#if !end}
      <div class="more">
        <Button size="sm" icon="plus" {loading} onclick={loadMore}>{t('creator.loadMore')}</Button>
      </div>
    {/if}
    <DownloadBar
      filters="files"
      label={t('selection.download', { count: selected.size })}
      disabled={selected.size === 0}
      {busy}
      ondownload={download}
    />
  {/if}
</section>

<style>
  .listing {
    display: grid;
    gap: 14px;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    gap: 16px;
    flex-wrap: wrap;
  }
  h2 {
    font-size: var(--fs-xl);
    font-weight: 700;
  }
  form {
    display: flex;
    gap: 8px;
    align-items: center;
    min-width: 320px;
  }
  form :global(> :first-child) {
    flex: 1;
  }
  .bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: var(--fs-sm);
  }
  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 80px 0;
    color: var(--text-dim);
  }
  .more {
    display: flex;
    justify-content: center;
  }
</style>
