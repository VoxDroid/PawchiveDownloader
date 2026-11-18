<script lang="ts">
  import type { Post } from '../../lib/types';
  import { CATEGORY_ICON } from '../../lib/text';
  import { fmt, t } from '../../ui/state/app.svelte';
  import Checkbox from '../../ui/components/Checkbox.svelte';
  import Icon from '../../ui/components/Icon.svelte';
  import IconButton from '../../ui/components/IconButton.svelte';

  interface Props {
    post: Post;
    selected: boolean;
    downloaded: boolean;
    showCreator?: boolean;
    onselect: (event: MouseEvent | KeyboardEvent) => void;
    onopen: () => void;
  }

  let { post, selected, downloaded, showCreator = false, onselect, onopen }: Props = $props();

  const thumb = $derived(post.files.find((f) => f.thumbnailUrl)?.thumbnailUrl ?? null);
  const kinds = $derived([...new Set(post.files.map((f) => f.category))].slice(0, 4));
  const previews = $derived(post.files.filter((f) => f.previewOnly).length);
  let failed = $state(false);
</script>

<div
  class="row"
  class:selected
  role="option"
  aria-selected={selected}
  tabindex="0"
  onclick={onselect}
  ondblclick={onopen}
  onkeydown={(e) => {
    if (e.key === ' ') {
      e.preventDefault();
      onselect(e);
    } else if (e.key === 'Enter') onopen();
  }}
>
  <span class="check" aria-hidden="true">
    <Checkbox checked={selected} label={post.title || post.id} />
  </span>
  <div class="thumb">
    {#if thumb && !failed}
      <img src={thumb} alt="" loading="lazy" decoding="async" onerror={() => (failed = true)} />
    {:else}
      <Icon name={post.files.length ? 'image' : 'text'} />
    {/if}
  </div>
  <div class="main">
    <span class="title truncate">{post.title || t('common.untitled')}</span>
    <span class="sub truncate mono-num">
      {#if showCreator}{post.creatorId}<span class="sep">/</span>{/if}
      {post.published ? fmt.date(post.published) : '-'}
      <span class="sep">/</span>{t('post.files', { count: post.files.length })}
      {#each kinds as kind (kind)}<Icon name={CATEGORY_ICON[kind]} />{/each}
    </span>
  </div>
  <div class="badges">
    {#if downloaded}
      <span class="badge ok"><Icon name="check" />{t('file.downloaded')}</span>
    {/if}
    {#if previews}
      <span class="badge warn" data-tip={t('file.previewHint')}>
        <Icon name="eyeOff" />{previews}
      </span>
    {/if}
  </div>
  <IconButton
    size="sm"
    icon="chevronRight"
    label={t('creator.openPost')}
    tipSide="left"
    onclick={(e) => {
      e.stopPropagation();
      onopen();
    }}
  />
</div>

<style>
  .row {
    height: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 10px 0 12px;
    border-radius: var(--radius-md);
    cursor: pointer;
    outline: none;
    transition: background-color var(--dur-fast) var(--ease);
  }
  .row:hover {
    background: var(--bg-hover);
  }
  .row:focus-visible {
    box-shadow: inset 0 0 0 2px color-mix(in srgb, var(--accent) 60%, transparent);
  }
  .selected {
    background: var(--accent-softer);
  }
  .selected:hover {
    background: var(--accent-soft);
  }
  .check {
    pointer-events: none;
    display: flex;
  }
  .thumb {
    width: 40px;
    height: 40px;
    flex: none;
    border-radius: 9px;
    overflow: hidden;
    display: grid;
    place-items: center;
    background: var(--bg-elev-2);
    color: var(--text-faint);
  }
  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    animation: pdl-fade-in var(--dur) var(--ease);
  }
  .main {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 2px;
  }
  .title {
    font-weight: 600;
  }
  .sub {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: var(--fs-xs);
    color: var(--text-dim);
  }
  .sub :global(.icon) {
    color: var(--text-faint);
  }
  .sep {
    margin: 0 3px;
    color: var(--text-faint);
  }
  .badges {
    display: flex;
    gap: 6px;
    flex: none;
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border-radius: 99px;
    font-size: var(--fs-xs);
    font-weight: 600;
  }
  .ok {
    color: var(--success);
    background: color-mix(in srgb, var(--success) 14%, transparent);
  }
  .warn {
    color: var(--warning);
    background: color-mix(in srgb, var(--warning) 14%, transparent);
  }
</style>
