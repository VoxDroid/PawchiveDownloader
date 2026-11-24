<script lang="ts">
  import { onMount } from 'svelte';
  import { fade } from 'svelte/transition';
  import { api } from '../../lib/api';
  import { mediaSrc } from '../../lib/media';
  import type { Gallery, MediaCreator, MediaFile, MediaPost } from '../../lib/types';
  import { pageUrls } from '../../lib/urls';
  import { fmt, t } from '../../ui/state/app.svelte';
  import { describeError } from '../../ui/state/errors';
  import { go, openLink } from '../../ui/state/router.svelte';
  import { toast } from '../../ui/state/toasts.svelte';
  import Button from '../../ui/components/Button.svelte';
  import Empty from '../../ui/components/Empty.svelte';
  import Icon from '../../ui/components/Icon.svelte';
  import IconButton from '../../ui/components/IconButton.svelte';
  import Segmented from '../../ui/components/Segmented.svelte';
  import Select from '../../ui/components/Select.svelte';
  import Spinner from '../../ui/components/Spinner.svelte';
  import TextInput from '../../ui/components/TextInput.svelte';
  import PageHeader from '../../ui/layout/PageHeader.svelte';
  import LibraryViewer, { type ViewerItem } from './LibraryViewer.svelte';
  import VideoThumb from './VideoThumb.svelte';

  type Kind = 'all' | 'image' | 'video';
  type Sort = 'newest' | 'oldest' | 'title';
  type Size = 's' | 'm' | 'l';
  const SIZES: Record<Size, number> = { s: 120, m: 170, l: 240 };
  const PAGE = 16;

  const remembered = <T extends string>(key: string, fallback: T, allowed: readonly T[]): T => {
    try {
      const value = localStorage.getItem(key) as T | null;
      return value && allowed.includes(value) ? value : fallback;
    } catch {
      return fallback;
    }
  };
  const remember = (key: string, value: string) => {
    try {
      localStorage.setItem(key, value);
    } catch {
      // Only a convenience.
    }
  };

  let gallery = $state<Gallery | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(false);
  let creator = $state<string>('all');
  let kind = $state<Kind>('all');
  let sort = $state<Sort>(remembered('pdl.library.sort', 'newest', ['newest', 'oldest', 'title']));
  let size = $state<Size>(remembered('pdl.library.size', 'm', ['s', 'm', 'l']));
  let query = $state('');
  let limit = $state(PAGE);
  let viewing = $state<number | null>(null);
  let sentinel = $state<HTMLElement>();

  async function load() {
    loading = true;
    try {
      gallery = await api.libraryMedia();
      error = null;
    } catch (e) {
      error = describeError(e);
    } finally {
      loading = false;
    }
  }
  onMount(load);

  const words = $derived(query.trim().toLowerCase().split(/\s+/).filter(Boolean));
  const matches = (post: MediaPost) => {
    const text = `${post.title} ${post.creatorName}`.toLowerCase();
    return words.every((w) => text.includes(w));
  };

  /** The posts on show, with only the files of the chosen kind. */
  const posts = $derived.by(() => {
    if (!gallery) return [];
    const list = gallery.posts
      .filter((p) => creator === 'all' || `${p.service}:${p.creatorId}` === creator)
      .filter(matches)
      .map((p) => (kind === 'all' ? p : { ...p, files: p.files.filter((f) => f.kind === kind) }))
      .filter((p) => p.files.length > 0);
    if (sort === 'oldest') list.reverse();
    else if (sort === 'title') list.sort((a, b) => a.title.localeCompare(b.title));
    return list;
  });
  const shown = $derived(posts.slice(0, limit));
  /** Every file on show, in order, for moving through them in the viewer. */
  const items = $derived<ViewerItem[]>(
    posts.flatMap((post) => post.files.map((file) => ({ file, post }))),
  );
  const indexOf = $derived(new Map(items.map((item, i) => [item.file.hash, i])));
  const creators = $derived(
    (gallery?.creators ?? []).filter(
      (c) =>
        !words.length || words.every((w) => c.name.toLowerCase().includes(w)) || creator === c.key,
    ),
  );
  const current = $derived(gallery?.creators.find((c) => c.key === creator) ?? null);

  // A new filter starts at the top of the list.
  $effect(() => {
    void [creator, kind, sort, query];
    limit = PAGE;
  });
  $effect(() => remember('pdl.library.sort', sort));
  $effect(() => remember('pdl.library.size', size));

  // More posts as the end of the list comes into view.
  $effect(() => {
    const el = sentinel;
    if (!el) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting) && limit < posts.length) limit += PAGE;
      },
      { rootMargin: '600px' },
    );
    io.observe(el);
    return () => io.disconnect();
  });

  const initials = (name: string) =>
    name
      .split(/\s+/)
      .map((w) => w.match(/[\p{L}\p{N}]/u)?.[0] ?? '')
      .filter(Boolean)
      .slice(0, 2)
      .join('')
      .toUpperCase() || '?';
  const hue = (text: string) => [...text].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7);
  const siteName = (site: string) => site.charAt(0).toUpperCase() + site.slice(1);

  async function showFolder(post: MediaPost) {
    try {
      await api.openPath(post.folder);
    } catch (e) {
      toast(describeError(e), 'error');
    }
  }
</script>

<PageHeader title={t('library.title')} description={t('library.description')}>
  {#snippet actions()}
    <Button variant="ghost" icon="retry" {loading} onclick={load}>
      {t('library.refresh')}
    </Button>
  {/snippet}
</PageHeader>

{#if error}
  <Empty icon="error" title={t('library.failed')} body={error}>
    <Button icon="retry" onclick={load}>{t('common.retry')}</Button>
  </Empty>
{:else if !gallery}
  <div class="loading"><Spinner size={22} /></div>
{:else if gallery.posts.length === 0}
  <Empty icon="thumbnails" title={t('library.emptyTitle')} body={t('library.emptyBody')}>
    <Button variant="primary" icon="compass" onclick={() => go('browse')}>{t('nav.browse')}</Button>
  </Empty>
{:else}
  <div class="summary" in:fade={{ duration: 160 }}>
    <span class="stat"><Icon name="image" />{t('library.images', { count: gallery.images })}</span>
    <span class="stat"><Icon name="video" />{t('library.videos', { count: gallery.videos })}</span>
    <span class="stat"><Icon name="harddisk" />{fmt.bytes(gallery.bytes)}</span>
    {#if gallery.missing}
      <button class="stat missing" onclick={() => go('stats')} data-tip={t('library.missingHint')}>
        <Icon name="warning" />{t('library.gone', { count: gallery.missing })}
      </button>
    {/if}
  </div>

  <div class="toolbar">
    <div class="search">
      <TextInput
        label={t('library.search')}
        placeholder={t('library.search')}
        icon="search"
        clearable
        bind:value={query}
      />
    </div>
    <Segmented
      label={t('library.kind')}
      size="sm"
      bind:value={kind}
      options={[
        { value: 'all', label: t('library.kindAll') },
        { value: 'image', label: t('library.kindImages') },
        { value: 'video', label: t('library.kindVideos') },
      ]}
    />
    <Select
      label={t('library.sort')}
      value={sort}
      onchange={(v) => (sort = v as Sort)}
      options={[
        { value: 'newest', label: t('library.sortNewest') },
        { value: 'oldest', label: t('library.sortOldest') },
        { value: 'title', label: t('library.sortTitle') },
      ]}
    />
    <Segmented
      label={t('library.size')}
      size="sm"
      bind:value={size}
      options={[
        { value: 's', label: 'S' },
        { value: 'm', label: 'M' },
        { value: 'l', label: 'L' },
      ]}
    />
  </div>

  <div class="layout">
    <aside class="creators" aria-label={t('library.creators')}>
      <button class="creator" class:on={creator === 'all'} onclick={() => (creator = 'all')}>
        <span class="avatar all"><Icon name="thumbnails" /></span>
        <span class="who">
          <strong>{t('library.allCreators')}</strong>
          <span class="faint">{t('library.posts', { count: gallery.posts.length })}</span>
        </span>
      </button>
      {#each creators as c (c.key)}
        {@render creatorRow(c)}
      {/each}
    </aside>

    <section class="posts" style:--tile="{SIZES[size]}px">
      {#if current}
        <header class="current" in:fade={{ duration: 140 }}>
          <div>
            <h2>{current.name}</h2>
            <span class="faint">
              {siteName(current.site)} · {t('library.posts', { count: current.posts })} ·
              {fmt.bytes(current.bytes)}
            </span>
          </div>
          <Button
            size="sm"
            variant="secondary"
            icon="compass"
            onclick={() => openLink(pageUrls.creator(current.service, current.creatorId))}
          >
            {t('common.openInBrowse')}
          </Button>
        </header>
      {/if}

      {#if posts.length === 0}
        <Empty compact icon="search" title={t('library.noMatch')} body={t('library.noMatchBody')} />
      {/if}

      {#each shown as post (post.key)}
        <article class="post">
          <header>
            <div class="title">
              <h3 class="truncate selectable">{post.title}</h3>
              <span class="faint truncate">
                {[
                  creator === 'all' ? post.creatorName : '',
                  siteName(post.site),
                  fmt.date(post.at),
                  t('library.files', { count: post.files.length }),
                ]
                  .filter(Boolean)
                  .join(' · ')}
              </span>
            </div>
            <IconButton
              icon="compass"
              label={t('common.openInBrowse')}
              onclick={() => openLink(pageUrls.post(post.service, post.creatorId, post.postId))}
            />
            <IconButton
              icon="folderOpen"
              label={t('library.showFolder')}
              tipSide="left"
              onclick={() => showFolder(post)}
            />
          </header>
          <div class="grid">
            {#each post.files as file (file.hash)}
              {@render tile(file)}
            {/each}
          </div>
        </article>
      {/each}
      {#if shown.length < posts.length}
        <div class="more" bind:this={sentinel}>
          <Button variant="ghost" onclick={() => (limit += PAGE)}>{t('library.more')}</Button>
        </div>
      {/if}
    </section>
  </div>

  <LibraryViewer {items} index={viewing} onindex={(i) => (viewing = i)} />
{/if}

{#snippet creatorRow(c: MediaCreator)}
  <button class="creator" class:on={creator === c.key} onclick={() => (creator = c.key)}>
    <span class="avatar" style:--h={hue(c.name)}>
      <span>{initials(c.name)}</span>
      <img
        src={pageUrls.icon(c.service, c.creatorId)}
        alt=""
        loading="lazy"
        onerror={(e) => (e.currentTarget as HTMLImageElement).remove()}
      />
    </span>
    <span class="who">
      <strong class="truncate">{c.name}</strong>
      <span class="faint truncate">
        {siteName(c.site)} · {t('library.files', { count: c.images + c.videos })}
      </span>
    </span>
  </button>
{/snippet}

{#snippet tile(file: MediaFile)}
  {@const index = indexOf.get(file.hash) ?? 0}
  <button
    class="tile"
    class:video={file.kind === 'video'}
    aria-label={file.name}
    onclick={() => (viewing = index)}
  >
    {#if file.kind === 'image'}
      <img src={mediaSrc(file, true)} alt="" loading="lazy" decoding="async" />
    {:else}
      <VideoThumb src={mediaSrc(file)} />
      <span class="badge"><Icon name="play" size={14} />{t('library.video')}</span>
    {/if}
    <span class="name truncate">{file.name}</span>
  </button>
{/snippet}

<style>
  .loading {
    display: grid;
    place-items: center;
    padding: 80px 0;
  }
  .summary {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-bottom: 14px;
  }
  .stat {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 5px 12px;
    border-radius: 99px;
    border: 1px solid var(--border);
    background: var(--bg-elev);
    color: var(--text-dim);
    font: inherit;
    font-size: var(--fs-sm);
  }
  .stat :global(.icon) {
    color: var(--accent-strong);
  }
  .stat.missing {
    border-color: color-mix(in srgb, var(--warning) 45%, var(--border));
    color: var(--text);
  }
  .stat.missing :global(.icon) {
    color: var(--warning);
  }
  .stat.missing:hover {
    background: var(--bg-elev-2);
  }
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin-bottom: 18px;
  }
  .search {
    flex: 1;
    min-width: 220px;
    max-width: 380px;
  }
  .layout {
    display: grid;
    grid-template-columns: 230px minmax(0, 1fr);
    gap: 22px;
    align-items: start;
  }
  @media (max-width: 980px) {
    .layout {
      grid-template-columns: minmax(0, 1fr);
    }
    .creators {
      position: static;
      max-height: 220px;
    }
  }
  .creators {
    position: sticky;
    top: 0;
    display: grid;
    gap: 2px;
    max-height: calc(100vh - 120px);
    overflow-y: auto;
    padding: 6px;
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    background: var(--bg-elev);
  }
  .creator {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 7px 8px;
    border: 0;
    border-radius: var(--radius-md);
    background: none;
    color: var(--text);
    text-align: left;
    font: inherit;
    transition: background-color var(--dur-fast) var(--ease);
  }
  .creator:hover {
    background: var(--bg-elev-2);
  }
  .creator.on {
    background: var(--accent-soft);
  }
  .avatar {
    position: relative;
    flex: none;
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 50%;
    overflow: hidden;
    background: hsl(var(--h, 260) 45% 32%);
    color: #fff;
    font-size: var(--fs-xs);
    font-weight: 700;
  }
  .avatar img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .avatar.all {
    background: var(--bg-elev-3);
    color: var(--accent-strong);
  }
  .who {
    display: grid;
    gap: 1px;
    min-width: 0;
  }
  .who strong {
    font-size: var(--fs-sm);
  }
  .who span {
    font-size: var(--fs-xs);
  }
  .posts {
    display: grid;
    gap: 26px;
    min-width: 0;
  }
  .current {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding-bottom: 14px;
    border-bottom: 1px solid var(--border);
  }
  .current h2 {
    font-size: var(--fs-xl);
    font-weight: 700;
  }
  .current span {
    font-size: var(--fs-sm);
  }
  .post header {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 10px;
  }
  .post .title {
    display: grid;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }
  .post h3 {
    font-size: var(--fs-md);
    font-weight: 700;
  }
  .post .title span {
    font-size: var(--fs-xs);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(var(--tile), 1fr));
    gap: 8px;
  }
  .tile {
    position: relative;
    aspect-ratio: 1;
    padding: 0;
    overflow: hidden;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: var(--bg-elev-2);
    cursor: zoom-in;
    transition:
      border-color var(--dur-fast) var(--ease),
      transform var(--dur) var(--ease-spring);
  }
  .tile:hover {
    border-color: color-mix(in srgb, var(--accent) 60%, var(--border));
    transform: translateY(-2px);
  }
  .tile img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .badge {
    position: absolute;
    top: 6px;
    left: 6px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px 2px 6px;
    border-radius: 99px;
    background: rgb(0 0 0 / 0.62);
    color: #fff;
    font-size: var(--fs-xs);
    font-weight: 600;
    pointer-events: none;
  }
  .name {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    padding: 18px 8px 6px;
    background: linear-gradient(transparent, rgb(0 0 0 / 0.7));
    color: #fff;
    font-size: var(--fs-xs);
    text-align: left;
    opacity: 0;
    transition: opacity var(--dur-fast) var(--ease);
    pointer-events: none;
  }
  .tile:hover .name,
  .tile:focus-visible .name {
    opacity: 1;
  }
  .more {
    display: grid;
    place-items: center;
    padding: 10px 0 30px;
  }
</style>
