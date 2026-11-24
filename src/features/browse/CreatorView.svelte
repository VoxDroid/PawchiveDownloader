<script lang="ts">
  import { serviceLabel } from '../../lib/urls';
  import { describeError } from '../../ui/state/errors';
  import { fade } from 'svelte/transition';
  import { api, openUrl } from '../../lib/api';
  import type { Creator, CreatorView, JobOverrides, Post } from '../../lib/types';
  import { fmt, t } from '../../ui/state/app.svelte';
  import { toast } from '../../ui/state/toasts.svelte';
  import { openLink } from '../../ui/state/router.svelte';
  import Avatar from '../../ui/components/Avatar.svelte';
  import Button from '../../ui/components/Button.svelte';
  import Chip from '../../ui/components/Chip.svelte';
  import Empty from '../../ui/components/Empty.svelte';
  import Icon from '../../ui/components/Icon.svelte';
  import IconButton from '../../ui/components/IconButton.svelte';
  import Spinner from '../../ui/components/Spinner.svelte';
  import TextInput from '../../ui/components/TextInput.svelte';
  import DownloadBar from './DownloadBar.svelte';
  import { queueJob, sampleFile } from './queue';
  import PostList from './PostList.svelte';
  import { rememberRecent } from './batch.svelte';

  interface Props {
    service: string;
    creatorId: string;
    initialQuery: string | null;
    /** Called with the creator's name once it is known. */
    ontitle?: (title: string) => void;
  }

  let { service, creatorId, initialQuery, ontitle }: Props = $props();
  const PAGE = 50;

  let view = $state<CreatorView | null>(null);
  let error = $state<string | null>(null);
  let posts = $state<Post[]>([]);
  let downloaded = $state(new Set<string>());
  let opened = $state(new Set<string>());
  let selected = $state(new Set<string>());
  let offset = $state(0);
  let end = $state(false);
  let loading = $state(false);
  let loadingAll = $state(false);
  let stopAll = false;
  let query = $state('');
  let appliedQuery = $state<string | null>(null);
  let titleFilter = $state('');
  let from = $state('');
  let to = $state('');
  let onlyFiles = $state(false);
  let hideDownloaded = $state(false);
  let busy = $state(false);
  let linked = $state<Creator[]>([]);

  const shown = $derived(
    posts.filter((p) => {
      const date = (p.published ?? '').slice(0, 10);
      if (from && date && date < from) return false;
      if (to && date && date > to) return false;
      if (onlyFiles && p.files.length === 0) return false;
      if (hideDownloaded && downloaded.has(p.id)) return false;
      const needle = titleFilter.trim().toLowerCase();
      if (needle && !p.title.toLowerCase().includes(needle) && !p.id.includes(needle)) return false;
      return true;
    }),
  );
  const selectedFiles = $derived(
    posts.filter((p) => selected.has(p.id)).reduce((n, p) => n + p.files.length, 0),
  );
  const filtered = $derived(
    Boolean(from || to || onlyFiles || hideDownloaded || titleFilter.trim()),
  );

  async function loadCreator() {
    error = null;
    view = null;
    posts = [];
    offset = 0;
    end = false;
    selected = new Set();
    linked = [];
    query = initialQuery ?? '';
    appliedQuery = initialQuery;
    try {
      view = await api.getCreator(service, creatorId);
      rememberRecent({
        url: view.creator.url,
        title: view.creator.name,
        kind: 'creator',
        at: Date.now(),
      });
      ontitle?.(view.creator.name);
      await loadMore();
      // Other accounts of the same creator; quietly absent when unavailable.
      const forId = creatorId;
      void api
        .linkedAccounts(service, creatorId)
        .then((list) => {
          if (forId === creatorId) linked = list;
        })
        .catch(() => {});
    } catch (e) {
      error = describeError(e);
    }
  }

  $effect(() => {
    void [service, creatorId, initialQuery];
    void loadCreator();
  });

  async function loadMore(): Promise<boolean> {
    if (loading || end) return false;
    loading = true;
    try {
      const page = await api.getCreatorPosts(service, creatorId, offset, appliedQuery);
      const seen = new Set(posts.map((p) => p.id));
      posts = [...posts, ...page.posts.filter((p) => !seen.has(p.id))];
      downloaded = new Set([...downloaded, ...page.downloaded]);
      opened = new Set([...opened, ...(page.seen ?? [])]);
      offset += PAGE;
      if (page.posts.length < PAGE) end = true;
      return true;
    } catch (e) {
      toast(describeError(e), 'error');
      return false;
    } finally {
      loading = false;
    }
  }

  async function loadAll() {
    loadingAll = true;
    stopAll = false;
    while (!end && !stopAll) {
      if (!(await loadMore())) break;
    }
    loadingAll = false;
  }

  async function search() {
    stopAll = true;
    appliedQuery = query.trim() || null;
    posts = [];
    offset = 0;
    end = false;
    selected = new Set();
    await loadMore();
  }

  function selectShown(on: boolean) {
    const next = new Set(selected);
    for (const p of shown) {
      if (on) next.add(p.id);
      else next.delete(p.id);
    }
    selected = next;
  }

  async function toggleWatch() {
    if (!view) return;
    try {
      if (view.watched) {
        await api.removeWatch([`${service}:${creatorId}`]);
        view.watched = false;
        toast(t('watch.removed', { name: view.creator.name }), 'info');
      } else {
        await api.addWatch(service, creatorId, view.creator.name);
        view.watched = true;
        toast(t('watch.added', { name: view.creator.name }), 'success');
      }
    } catch (e) {
      toast(describeError(e), 'error');
    }
  }

  async function toggleFavorite() {
    if (!view) return;
    try {
      if (view.favorite) {
        await api.removeFavorites([`c:${service}:${creatorId}`]);
        view.favorite = false;
        toast(t('favorites.removed'), 'info');
      } else {
        await api.addFavorite({ service, creatorId, title: view.creator.name });
        view.favorite = true;
        toast(t('favorites.added'), 'success');
      }
    } catch (e) {
      toast(describeError(e), 'error');
    }
  }

  async function download(options: JobOverrides) {
    if (!view) return;
    busy = true;
    const clientFiltered = onlyFiles || hideDownloaded || Boolean(titleFilter.trim());
    const ids = selected.size
      ? posts.filter((p) => selected.has(p.id)).map((p) => p.id)
      : clientFiltered
        ? shown.map((p) => p.id)
        : null;
    if (ids && ids.length === 0) {
      busy = false;
      toast(t('creator.noMatches'), 'warning');
      return;
    }
    const queued = await queueJob(
      {
        kind: 'creator',
        service,
        creatorId,
        query: appliedQuery,
        postIds: ids ?? undefined,
        since: !ids && from ? `${from}T00:00:00` : null,
        until: !ids && to ? `${to}T23:59:59` : null,
        options,
      },
      ids
        ? t('toast.queuedPosts', { count: ids.length })
        : t('toast.queuedCreator', { name: view.creator.name }),
      sampleFile(ids ? posts.filter((p) => ids.includes(p.id)) : posts),
    );
    if (queued) selected = new Set();
    busy = false;
  }
</script>

{#if error}
  <Empty
    icon="cloudOff"
    title={t('creator.loadFailed')}
    body={error === 'not-found' ? t('error.notFound') : error}
  >
    <Button icon="retry" onclick={loadCreator}>{t('common.retry')}</Button>
  </Empty>
{:else if !view}
  <div class="loading"><Spinner size={22} /><span>{t('common.loading')}</span></div>
{:else}
  {@const creator = view.creator}
  <section class="creator" in:fade={{ duration: 200 }}>
    <header class="hero">
      <img
        class="banner"
        src={creator.bannerUrl}
        alt=""
        onerror={(e) => ((e.currentTarget as HTMLImageElement).style.opacity = '0')}
      />
      <div class="hero-row">
        <span class="avatar">
          <Avatar src={creator.iconUrl} name={creator.name} size={76} radius="18px" />
        </span>
        <div class="who">
          <h2 class="selectable">{creator.name}</h2>
          <p>
            {serviceLabel(creator.service)}<span class="sep">/</span><span class="mono-num">
              {creator.id}
            </span>
            {#if creator.updated}<span class="sep">/</span>{t('creator.updated', {
                date: fmt.date(creator.updated),
              })}{/if}
            {#if creator.postCount}<span class="sep">/</span>{t('creator.postCount', {
                count: creator.postCount,
              })}{/if}
          </p>
          {#if linked.length}
            <div class="linked">
              <span class="faint">{t('creator.alsoOn')}</span>
              {#each linked as other (`${other.service}:${other.id}`)}
                <button class="link-chip" onclick={() => openLink(other.url)} title={other.url}>
                  <Avatar src={other.iconUrl} name={other.name} size={20} radius="99px" />
                  <span class="truncate">{other.name}</span>
                  <span class="faint">{serviceLabel(other.service)}</span>
                </button>
              {/each}
            </div>
          {/if}
        </div>
        <div class="hero-actions">
          <Button
            size="sm"
            variant={view.watched ? 'subtle' : 'secondary'}
            icon={view.watched ? 'bellRing' : 'bell'}
            onclick={toggleWatch}
          >
            {view.watched ? t('creator.watching') : t('creator.watch')}
          </Button>
          <IconButton
            icon={view.favorite ? 'star' : 'starOutline'}
            active={view.favorite}
            label={view.favorite ? t('favorites.remove') : t('favorites.add')}
            onclick={toggleFavorite}
          />
          <IconButton
            icon="external"
            label={t('browse.openSite')}
            tipSide="left"
            onclick={() => openUrl(creator.url)}
          />
        </div>
      </div>
    </header>

    <div class="toolbar">
      <form
        class="search"
        onsubmit={(e) => {
          e.preventDefault();
          void search();
        }}
      >
        <TextInput
          label={t('creator.searchSite')}
          placeholder={t('creator.searchSite')}
          icon="search"
          bind:value={query}
          clearable
        />
        <Button size="sm" type="submit">{t('common.search')}</Button>
      </form>
      <div class="range">
        <TextInput label={t('creator.from')} type="date" bind:value={from} />
        <span class="faint">-</span>
        <TextInput label={t('creator.to')} type="date" bind:value={to} />
      </div>
    </div>

    <div class="filters">
      <TextInput
        label={t('creator.search')}
        placeholder={t('creator.search')}
        icon="filter"
        bind:value={titleFilter}
        clearable
      />
      <Chip
        icon="file"
        label={t('creator.onlyFiles')}
        active={onlyFiles}
        onclick={() => (onlyFiles = !onlyFiles)}
      />
      {#if downloaded.size}
        <Chip
          icon="eyeOff"
          label={t('creator.hideDownloaded')}
          count={downloaded.size}
          active={hideDownloaded}
          onclick={() => (hideDownloaded = !hideDownloaded)}
        />
      {/if}
      <span class="spacer"></span>
      <span class="mono-num faint count">
        {t('creator.loaded', { shown: shown.length, total: posts.length })}
      </span>
      <IconButton
        size="sm"
        icon="selectAll"
        label={t('common.selectAll')}
        onclick={() => selectShown(true)}
      />
      <IconButton
        size="sm"
        icon="selectNone"
        label={t('common.selectNone')}
        tipSide="left"
        onclick={() => selectShown(false)}
      />
    </div>

    <PostList
      posts={shown}
      {selected}
      {downloaded}
      seen={opened}
      onchange={(next) => (selected = next)}
    />

    <div class="more">
      {#if loadingAll}
        <span class="faint mono-num">
          <Spinner size={14} />
          {t('creator.loadingAll', { count: posts.length })}
        </span>
        <Button size="sm" variant="ghost" icon="stop" onclick={() => (stopAll = true)}>
          {t('common.stop')}
        </Button>
      {:else if !end}
        <Button size="sm" icon="plus" {loading} onclick={loadMore}>{t('creator.loadMore')}</Button>
        <Button size="sm" variant="ghost" icon="downloadMultiple" onclick={loadAll}>
          {t('creator.loadAll')}
        </Button>
      {:else}
        <span class="faint">
          <Icon name="checkAll" />
          {t('creator.allLoaded', { count: posts.length })}
        </span>
      {/if}
    </div>

    <DownloadBar
      filters="posts"
      label={selected.size
        ? t('creator.download', { count: selected.size })
        : filtered || appliedQuery
          ? t('creator.downloadMatching')
          : t('creator.downloadAll')}
      summary={selected.size ? t('creator.selectedFiles', { count: selectedFiles }) : undefined}
      {busy}
      ondownload={download}
    />
    {#if !selected.size && !end && (onlyFiles || hideDownloaded || titleFilter.trim())}
      <p class="hint faint"><Icon name="info" /> {t('creator.filterHint')}</p>
    {/if}
  </section>
{/if}

<style>
  .linked {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 6px;
    font-size: var(--fs-sm);
  }
  .link-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    max-width: 240px;
    height: 26px;
    padding: 0 10px 0 3px;
    border-radius: 99px;
    border: 1px solid var(--border);
    background: var(--bg-elev-2);
    color: var(--text);
    font-size: var(--fs-sm);
    transition: border-color var(--dur-fast) var(--ease);
  }
  .link-chip:hover {
    border-color: var(--accent);
  }
  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 80px 0;
    color: var(--text-dim);
  }
  .creator {
    display: grid;
    gap: 14px;
  }
  .hero {
    position: relative;
    border-radius: var(--radius-xl);
    overflow: hidden;
    border: 1px solid var(--border);
    background: var(--bg-elev);
  }
  .banner {
    display: block;
    width: 100%;
    height: 150px;
    object-fit: cover;
    background: linear-gradient(120deg, var(--accent-soft), var(--bg-elev-2));
    transition: opacity var(--dur) var(--ease);
  }
  /* Only the avatar reaches into the banner; the name and details sit
     below it, so they never cover the picture. */
  .hero-row {
    display: flex;
    align-items: flex-start;
    gap: 16px;
    padding: 0 18px 16px;
    position: relative;
  }
  /* The picture with a frame in the card's color, reaching into the banner. */
  .avatar {
    flex: none;
    display: block;
    margin-top: -42px;
    padding: 4px;
    border-radius: 22px;
    background: var(--bg-elev);
    box-shadow: var(--shadow-md);
    line-height: 0;
  }
  .who {
    flex: 1;
    min-width: 0;
    padding-top: 12px;
  }
  .who h2 {
    font-size: var(--fs-xl);
    font-weight: 700;
  }
  .who p {
    color: var(--text-dim);
    font-size: var(--fs-sm);
  }
  .sep {
    margin: 0 7px;
    color: var(--text-faint);
  }
  .hero-actions {
    display: flex;
    gap: 4px;
    align-items: center;
    padding-top: 12px;
  }
  .toolbar {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
  }
  .search {
    display: flex;
    gap: 8px;
    align-items: center;
    flex: 1;
    min-width: 280px;
    max-width: 520px;
  }
  .search :global(> :first-child) {
    flex: 1;
  }
  .range {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .filters {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .filters :global(> :first-child) {
    width: 240px;
  }
  .spacer {
    flex: 1;
  }
  .count {
    font-size: var(--fs-sm);
    margin-right: 4px;
  }
  .more {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 10px;
    font-size: var(--fs-sm);
  }
  .more span {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .hint {
    display: flex;
    gap: 6px;
    align-items: center;
    font-size: var(--fs-xs);
    margin-top: -8px;
  }
</style>
