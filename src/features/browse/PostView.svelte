<script lang="ts">
  import { describeError } from '../../ui/state/errors';
  import { fade } from 'svelte/transition';
  import { api, copyText, openUrl } from '../../lib/api';
  import { htmlToText } from '../../lib/text';
  import {
    FILE_CATEGORIES,
    type FileCategory,
    type JobOverrides,
    type PostView,
  } from '../../lib/types';
  import { app, fmt, t } from '../../ui/state/app.svelte';
  import { openLink } from '../../ui/state/router.svelte';
  import { toast } from '../../ui/state/toasts.svelte';
  import Button from '../../ui/components/Button.svelte';
  import Chip from '../../ui/components/Chip.svelte';
  import Empty from '../../ui/components/Empty.svelte';
  import Icon from '../../ui/components/Icon.svelte';
  import IconButton from '../../ui/components/IconButton.svelte';
  import Segmented from '../../ui/components/Segmented.svelte';
  import Spinner from '../../ui/components/Spinner.svelte';
  import DownloadBar from './DownloadBar.svelte';
  import { queueJob } from './queue';
  import FileTile from './FileTile.svelte';
  import { rememberRecent } from './batch.svelte';

  let { service, creatorId, postId }: { service: string; creatorId: string; postId: string } =
    $props();

  let view = $state<PostView | null>(null);
  let error = $state<string | null>(null);
  let selected = $state(new Set<number>());
  let filter = $state<Set<FileCategory>>(new Set());
  let hideDownloaded = $state(false);
  let tab = $state<'files' | 'text'>('files');
  let busy = $state(false);
  let favorite = $state(false);
  let lastIndex: number | null = null;

  const post = $derived(view?.post ?? null);
  const known = $derived(new Set(view?.known ?? []));
  const counts = $derived.by(() => {
    const result = new Map<FileCategory, number>();
    for (const file of post?.files ?? [])
      result.set(file.category, (result.get(file.category) ?? 0) + 1);
    return result;
  });
  const visible = $derived(
    (post?.files ?? []).filter(
      (f) =>
        (filter.size === 0 || filter.has(f.category)) &&
        !(hideDownloaded && f.hash && known.has(f.hash)),
    ),
  );
  const allVisibleSelected = $derived(
    visible.length > 0 && visible.every((f) => selected.has(f.index)),
  );
  const text = $derived(post ? htmlToText(post.contentHtml) : '');
  const previewOnly = $derived((post?.files ?? []).filter((f) => f.previewOnly).length);
  const creatorName = $derived(view?.creator?.name ?? post?.creatorId ?? '');

  async function load() {
    error = null;
    view = null;
    try {
      const loaded = await api.getPost(service, creatorId, postId);
      view = loaded;
      favorite = loaded.favorite;
      const fast = app.settings.fastMode;
      selected = new Set(
        loaded.post.files
          .filter((f) => fast || app.settings.categories[f.category])
          .map((f) => f.index),
      );
      rememberRecent({
        url: loaded.post.url,
        title: loaded.post.title || loaded.post.id,
        kind: 'post',
        at: Date.now(),
      });
    } catch (e) {
      error = describeError(e);
    }
  }

  $effect(() => {
    void [service, creatorId, postId];
    void load();
  });

  function toggleFilter(category: FileCategory) {
    const next = new Set(filter);
    if (next.has(category)) next.delete(category);
    else next.add(category);
    filter = next;
  }

  function onselect(index: number, event: MouseEvent) {
    const next = new Set(selected);
    const position = visible.findIndex((f) => f.index === index);
    if (event.shiftKey && lastIndex !== null) {
      const from = visible.findIndex((f) => f.index === lastIndex);
      const [a, b] = from < position ? [from, position] : [position, from];
      const on = !selected.has(index);
      for (const f of visible.slice(a, b + 1)) {
        if (on) next.add(f.index);
        else next.delete(f.index);
      }
    } else if (next.has(index)) next.delete(index);
    else next.add(index);
    lastIndex = index;
    selected = next;
  }

  function selectVisible(on: boolean) {
    const next = new Set(selected);
    for (const f of visible) {
      if (on) next.add(f.index);
      else next.delete(f.index);
    }
    selected = next;
  }

  function invert() {
    const next = new Set<number>();
    for (const f of post?.files ?? []) if (!selected.has(f.index)) next.add(f.index);
    selected = next;
  }

  async function download(options: JobOverrides) {
    if (!post) return;
    busy = true;
    const fileIndexes = [...selected].sort((a, b) => a - b);
    await queueJob(
      {
        kind: 'posts',
        title: post.title || post.id,
        creator: { service: post.service, id: post.creatorId, name: creatorName },
        posts: [{ post, fileIndexes }],
        options,
      },
      fileIndexes.length
        ? t('toast.queuedFiles', { count: fileIndexes.length })
        : t('toast.queuedPost'),
    );
    busy = false;
  }

  async function toggleFavorite() {
    if (!post) return;
    try {
      if (favorite) {
        await api.removeFavorites([`p:${post.service}:${post.creatorId}:${post.id}`]);
        favorite = false;
        toast(t('favorites.removed'), 'info');
      } else {
        await api.addFavorite({
          service: post.service,
          creatorId: post.creatorId,
          postId: post.id,
          title: post.title || post.id,
          creatorName,
        });
        favorite = true;
        toast(t('favorites.added'), 'success');
      }
    } catch (e) {
      toast(describeError(e), 'error');
    }
  }

  async function copy() {
    await copyText(text);
    toast(t('common.copied'), 'success');
  }
</script>

{#if error}
  <Empty
    icon="cloudOff"
    title={t('post.loadFailed')}
    body={error === 'not-found' ? t('error.notFound') : error}
  >
    <Button icon="retry" onclick={load}>{t('common.retry')}</Button>
  </Empty>
{:else if !post || !view}
  <div class="loading"><Spinner size={22} /><span>{t('post.loading')}</span></div>
{:else}
  <article class="post" in:fade={{ duration: 200 }}>
    <header class="intro">
      <button
        class="avatar-btn"
        onclick={() => openLink(`https://pawchive.pw/${post.service}/user/${post.creatorId}`)}
        data-tip={t('browse.openCreator')}
      >
        <img class="avatar" src={view.creator?.iconUrl ?? ''} alt="" loading="lazy" />
      </button>
      <div class="meta">
        <h2 class="title selectable" title={post.title}>{post.title || t('common.untitled')}</h2>
        <p class="sub">
          <button
            class="crumb"
            onclick={() => openLink(`https://pawchive.pw/${post.service}/user/${post.creatorId}`)}
          >
            {creatorName}
          </button>
          <span class="sep">/</span>{post.service}
          {#if post.published}<span class="sep">/</span>{fmt.date(post.published)}{/if}
          <span class="sep">/</span>{t('post.files', { count: post.files.length })}
        </p>
        {#if post.tags.length}
          <div class="tags">
            {#each post.tags.slice(0, 12) as tag (tag)}<span class="tag">#{tag}</span>{/each}
          </div>
        {/if}
      </div>
      <div class="head-actions">
        {#if post.prevId}
          <IconButton
            icon="chevronLeft"
            label={t('browse.newerPost')}
            onclick={() =>
              openLink(
                `https://pawchive.pw/${post.service}/user/${post.creatorId}/post/${post.prevId}`,
              )}
          />
        {/if}
        {#if post.nextId}
          <IconButton
            icon="chevronRight"
            label={t('browse.olderPost')}
            onclick={() =>
              openLink(
                `https://pawchive.pw/${post.service}/user/${post.creatorId}/post/${post.nextId}`,
              )}
          />
        {/if}
        <IconButton
          icon={favorite ? 'star' : 'starOutline'}
          active={favorite}
          label={favorite ? t('favorites.remove') : t('favorites.add')}
          onclick={toggleFavorite}
        />
        <IconButton
          icon="external"
          label={t('browse.openSite')}
          tipSide="left"
          onclick={() => openUrl(post.url)}
        />
      </div>
    </header>

    <div class="tabs">
      <Segmented
        label={t('post.view')}
        size="sm"
        bind:value={tab}
        options={[
          { value: 'files', label: t('post.files', { count: post.files.length }), icon: 'layers' },
          { value: 'text', label: t('post.text'), icon: 'text' },
        ]}
      />
      {#if tab === 'files' && post.files.length}
        <div class="sel">
          <span class="mono-num faint">
            {t('post.selected', { count: selected.size, total: post.files.length })}
          </span>
          <IconButton
            size="sm"
            icon={allVisibleSelected ? 'selectNone' : 'selectAll'}
            label={allVisibleSelected ? t('common.selectNone') : t('common.selectAll')}
            onclick={() => selectVisible(!allVisibleSelected)}
          />
          <IconButton
            size="sm"
            icon="invert"
            label={t('common.invert')}
            tipSide="left"
            onclick={invert}
          />
        </div>
      {/if}
    </div>

    {#if tab === 'files'}
      {#if post.files.length === 0}
        <Empty compact icon="file" title={t('post.noFiles')} body={t('post.noFilesHint')} />
      {:else}
        <div class="filters">
          {#each FILE_CATEGORIES.filter((c) => counts.has(c)) as category (category)}
            <Chip
              label={t(`category.${category}`)}
              count={counts.get(category)}
              active={filter.has(category)}
              onclick={() => toggleFilter(category)}
            />
          {/each}
          {#if known.size}
            <Chip
              icon="eyeOff"
              label={t('post.hideDownloaded')}
              active={hideDownloaded}
              onclick={() => (hideDownloaded = !hideDownloaded)}
            />
          {/if}
        </div>
        {#if previewOnly}
          <p class="notice">
            <Icon name="info" />
            {app.settings.previewOnly === 'relay'
              ? t('post.previewNoticeRelay', { count: previewOnly })
              : t('post.previewNotice', { count: previewOnly })}
          </p>
        {/if}
        <div
          class="grid"
          role="listbox"
          aria-multiselectable="true"
          aria-label={t('post.filesLabel')}
        >
          {#each visible as file (file.index)}
            <FileTile
              {file}
              selected={selected.has(file.index)}
              downloaded={Boolean(file.hash && known.has(file.hash))}
              relay={file.previewOnly ? app.settings.previewOnly === 'relay' : null}
              onselect={(e) => onselect(file.index, e)}
            />
          {/each}
        </div>
      {/if}
    {:else if text}
      <div class="text-actions">
        <Button size="sm" variant="ghost" icon="copy" onclick={copy}>{t('common.copy')}</Button>
      </div>
      <pre class="text selectable">{text}</pre>
    {:else}
      <Empty compact icon="text" title={t('post.noText')} />
    {/if}

    <DownloadBar
      label={selected.size
        ? t('post.download', { count: selected.size })
        : t('post.downloadNothing')}
      disabled={selected.size === 0 && !text}
      {busy}
      ondownload={download}
    />
  </article>
{/if}

<style>
  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 80px 0;
    color: var(--text-dim);
  }
  .post {
    display: grid;
    gap: 16px;
  }
  .intro {
    display: flex;
    gap: 16px;
    align-items: flex-start;
  }
  .avatar-btn {
    padding: 0;
    border: 0;
    background: none;
    border-radius: 16px;
  }
  .avatar {
    width: 56px;
    height: 56px;
    border-radius: 16px;
    object-fit: cover;
    background: var(--bg-elev-2);
    border: 1px solid var(--border);
    display: block;
  }
  .meta {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 4px;
  }
  .title {
    font-size: var(--fs-xl);
    font-weight: 700;
    line-height: 1.3;
    overflow-wrap: anywhere;
  }
  .sub {
    color: var(--text-dim);
    font-size: var(--fs-sm);
  }
  .crumb {
    border: 0;
    background: none;
    padding: 0;
    color: var(--accent-strong);
    font-weight: 600;
  }
  .crumb:hover {
    text-decoration: underline;
  }
  .sep {
    margin: 0 7px;
    color: var(--text-faint);
  }
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 8px;
    margin-top: 2px;
  }
  .tag {
    font-size: var(--fs-xs);
    color: var(--text-faint);
  }
  .head-actions {
    display: flex;
    gap: 2px;
    flex: none;
  }
  .tabs {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }
  .sel {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: var(--fs-sm);
  }
  .sel span {
    margin-right: 6px;
  }
  .filters {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .notice {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    padding: 10px 12px;
    border-radius: var(--radius-md);
    background: var(--accent-softer);
    border: 1px solid color-mix(in srgb, var(--accent) 25%, transparent);
    color: var(--text-dim);
    font-size: var(--fs-sm);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 10px;
  }
  .text-actions {
    display: flex;
    justify-content: flex-end;
    margin-bottom: -8px;
  }
  .text {
    margin: 0;
    padding: 16px 18px;
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    background: var(--bg-elev);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font: inherit;
    font-size: var(--fs-md);
    line-height: 1.7;
    color: var(--text);
    max-height: 60vh;
    overflow-y: auto;
  }
</style>
