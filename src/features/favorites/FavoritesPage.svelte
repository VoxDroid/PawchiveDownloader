<script lang="ts">
  import { describeError } from '../../ui/state/errors';
  import { onMount } from 'svelte';
  import { flip } from 'svelte/animate';
  import { fade } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { api, pickFile, pickSavePath } from '../../lib/api';
  import { stamp } from '../../lib/files';
  import type { Favorite } from '../../lib/types';
  import { pageUrls } from '../../lib/urls';
  import { fmt, t } from '../../ui/state/app.svelte';
  import { go, openLink } from '../../ui/state/router.svelte';
  import { toast } from '../../ui/state/toasts.svelte';
  import Button from '../../ui/components/Button.svelte';
  import Checkbox from '../../ui/components/Checkbox.svelte';
  import Dialog from '../../ui/components/Dialog.svelte';
  import Empty from '../../ui/components/Empty.svelte';
  import Icon from '../../ui/components/Icon.svelte';
  import IconButton from '../../ui/components/IconButton.svelte';
  import Segmented from '../../ui/components/Segmented.svelte';
  import TextInput from '../../ui/components/TextInput.svelte';
  import PageHeader from '../../ui/layout/PageHeader.svelte';

  let items = $state<Favorite[] | null>(null);
  let kind = $state<'all' | 'creator' | 'post'>('all');
  let query = $state('');
  let link = $state('');
  let adding = $state(false);
  let selected = $state(new Set<string>());
  let editing = $state<Favorite | null>(null);
  let editTitle = $state('');
  let editNote = $state('');

  const shown = $derived(
    (items ?? []).filter((f) => {
      if (kind !== 'all' && f.kind !== kind) return false;
      const q = query.trim().toLowerCase();
      return !q || `${f.title} ${f.creatorName ?? ''} ${f.note}`.toLowerCase().includes(q);
    }),
  );

  async function load() {
    items = await api.listFavorites();
  }
  onMount(load);

  async function add() {
    if (!link.trim()) return;
    adding = true;
    try {
      const links = await api.parseLinks(link);
      let added = 0;
      for (const { ref } of links) {
        if (ref.kind === 'post') {
          const view = await api.getPost(ref.service, ref.creatorId, ref.postId).catch(() => null);
          await api.addFavorite({
            service: ref.service,
            creatorId: ref.creatorId,
            postId: ref.postId,
            title: view?.post.title || ref.postId,
            creatorName: view?.creator?.name ?? null,
          });
          added += 1;
        } else if (ref.kind === 'creator') {
          const view = await api.getCreator(ref.service, ref.creatorId).catch(() => null);
          await api.addFavorite({
            service: ref.service,
            creatorId: ref.creatorId,
            title: view?.creator.name ?? ref.creatorId,
          });
          added += 1;
        }
      }
      if (added) {
        toast(t('favorites.addedCount', { count: added }), 'success');
        link = '';
        await load();
      } else toast(t('favorites.invalid'), 'warning');
    } catch (e) {
      toast(describeError(e), 'error');
    } finally {
      adding = false;
    }
  }

  async function queue(keys: string[]) {
    try {
      const jobs = await api.queueFavorites(keys);
      toast(t('favorites.queued', { count: jobs.length }), 'success', {
        action: { label: t('nav.downloads'), run: () => go('downloads') },
      });
      selected = new Set();
      await load();
    } catch (e) {
      toast(describeError(e), 'error');
    }
  }

  async function remove(keys: string[]) {
    await api.removeFavorites(keys);
    selected = new Set([...selected].filter((k) => !keys.includes(k)));
    await load();
  }

  function edit(fav: Favorite) {
    editing = fav;
    editTitle = fav.title;
    editNote = fav.note;
  }

  async function saveEdit() {
    if (!editing) return;
    await api.updateFavorite(editing.key, editTitle, editNote);
    editing = null;
    await load();
  }

  async function exportList() {
    const path = await pickSavePath(t('common.export'), `pawchive-favorites-${stamp()}.txt`, [
      { name: 'Text', extensions: ['txt'] },
    ]);
    if (!path) return;
    const count = await api.exportFavorites(path);
    toast(t('favorites.exported', { count }), 'success');
  }

  async function importList() {
    const path = await pickFile(t('common.import'), [{ name: 'Text', extensions: ['txt'] }]);
    if (!path) return;
    try {
      const count = await api.importFavorites(path);
      toast(t('favorites.addedCount', { count }), count ? 'success' : 'info');
      await load();
    } catch (e) {
      toast(describeError(e), 'error');
    }
  }

  function url(f: Favorite) {
    return f.postId
      ? pageUrls.post(f.service, f.creatorId, f.postId)
      : pageUrls.creator(f.service, f.creatorId);
  }

  function toggle(key: string) {
    const next = new Set(selected);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    selected = next;
  }
</script>

<PageHeader title={t('nav.favorites')} description={t('favorites.description')}>
  {#snippet actions()}
    <Button icon="import" onclick={importList}>{t('common.import')}</Button>
    <Button icon="export" disabled={!items?.length} onclick={exportList}>
      {t('common.export')}
    </Button>
  {/snippet}
</PageHeader>

<form
  class="add"
  onsubmit={(e) => {
    e.preventDefault();
    void add();
  }}
>
  <TextInput
    label={t('favorites.addLabel')}
    placeholder={t('favorites.addPlaceholder')}
    icon="link"
    bind:value={link}
  />
  <Button variant="primary" type="submit" icon="star" loading={adding} disabled={!link.trim()}>
    {t('favorites.add')}
  </Button>
</form>

<div class="toolbar">
  <Segmented
    label={t('favorites.kind')}
    size="sm"
    bind:value={kind}
    options={[
      { value: 'all', label: t('history.all') },
      { value: 'creator', label: t('favorites.creators'), icon: 'account' },
      { value: 'post', label: t('favorites.posts'), icon: 'document' },
    ]}
  />
  <TextInput
    label={t('common.search')}
    placeholder={t('favorites.search')}
    icon="search"
    bind:value={query}
    clearable
  />
  {#if selected.size}
    <span class="bulk" in:fade={{ duration: 150 }}>
      <span class="faint mono-num">{t('selection.count', { count: selected.size })}</span>
      <Button size="sm" variant="primary" icon="download" onclick={() => queue([...selected])}>
        {t('favorites.downloadSelected')}
      </Button>
      <IconButton
        icon="trash"
        label={t('common.remove')}
        tipSide="left"
        onclick={() => remove([...selected])}
      />
    </span>
  {/if}
</div>

{#if items && items.length === 0}
  <Empty icon="starOutline" title={t('favorites.emptyTitle')} body={t('favorites.emptyBody')} />
{:else if items && shown.length === 0}
  <Empty compact icon="search" title={t('history.noMatches')} />
{:else}
  <div class="list">
    {#each shown as fav (fav.key)}
      <article
        class="row"
        class:selected={selected.has(fav.key)}
        animate:flip={{ duration: 220, easing: cubicOut }}
      >
        <Checkbox
          checked={selected.has(fav.key)}
          label={fav.title}
          onchange={() => toggle(fav.key)}
        />
        {#if fav.kind === 'creator'}
          <img
            class="avatar"
            src={pageUrls.icon(fav.service, fav.creatorId)}
            alt=""
            loading="lazy"
          />
        {:else}
          <span class="avatar post"><Icon name="document" /></span>
        {/if}
        <div class="main">
          <button class="title truncate" onclick={() => openLink(url(fav))}>{fav.title}</button>
          <span class="sub truncate">
            {fav.kind === 'creator' ? t('favorites.creator') : (fav.creatorName ?? fav.creatorId)}
            <span class="sep">/</span>{fav.service}
            <span class="sep">/</span>{t('favorites.addedAt', { date: fmt.date(fav.addedAt) })}
            {#if fav.lastDownloadedAt}<span class="sep">/</span>{t('favorites.lastDownloaded', {
                time: fmt.relative(fav.lastDownloadedAt),
              })}{/if}
          </span>
          {#if fav.note}<span class="note truncate"><Icon name="note" />{fav.note}</span>{/if}
        </div>
        <div class="actions">
          <IconButton
            icon="download"
            label={t('favorites.download')}
            onclick={() => queue([fav.key])}
          />
          <IconButton icon="pencil" label={t('favorites.edit')} onclick={() => edit(fav)} />
          <IconButton
            icon="trash"
            label={t('common.remove')}
            tipSide="left"
            onclick={() => remove([fav.key])}
          />
        </div>
      </article>
    {/each}
  </div>
{/if}

<Dialog
  open={editing !== null}
  title={t('favorites.edit')}
  confirmLabel={t('common.save')}
  onconfirm={saveEdit}
  oncancel={() => (editing = null)}
>
  <TextInput label={t('favorites.titleLabel')} bind:value={editTitle} />
  <textarea
    class="note-input selectable"
    rows="4"
    placeholder={t('favorites.notePlaceholder')}
    bind:value={editNote}></textarea>
</Dialog>

<style>
  .add {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-bottom: 18px;
  }
  .add :global(> :first-child) {
    flex: 1;
  }
  .toolbar {
    display: flex;
    gap: 12px;
    align-items: center;
    flex-wrap: wrap;
    margin-bottom: 14px;
  }
  .toolbar :global(.segmented) {
    width: 340px;
  }
  .toolbar :global(> :nth-child(2)) {
    width: 240px;
  }
  .bulk {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--fs-sm);
  }
  .list {
    display: grid;
    gap: 8px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    background: var(--bg-elev);
    transition:
      border-color var(--dur-fast) var(--ease),
      background-color var(--dur-fast) var(--ease);
  }
  .row:hover {
    border-color: var(--border-strong);
  }
  .row.selected {
    background: var(--accent-softer);
    border-color: color-mix(in srgb, var(--accent) 40%, var(--border));
  }
  .avatar {
    width: 40px;
    height: 40px;
    flex: none;
    border-radius: 11px;
    object-fit: cover;
    background: var(--bg-elev-2);
  }
  .avatar.post {
    display: grid;
    place-items: center;
    color: var(--accent-strong);
    background: var(--accent-softer);
  }
  .main {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 2px;
  }
  .title {
    border: 0;
    background: none;
    padding: 0;
    text-align: left;
    font-weight: 700;
    color: var(--text);
  }
  .title:hover {
    color: var(--accent-strong);
  }
  .sub,
  .note {
    font-size: var(--fs-xs);
    color: var(--text-dim);
  }
  .note {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--text-faint);
  }
  .sep {
    margin: 0 6px;
    color: var(--text-faint);
  }
  .actions {
    display: flex;
    gap: 2px;
  }
  .note-input {
    width: 100%;
    resize: vertical;
    padding: 10px 12px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border-strong);
    background: var(--bg);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-sm);
    outline: none;
  }
  .note-input:focus {
    border-color: var(--accent);
    box-shadow: var(--focus-ring);
  }
</style>
