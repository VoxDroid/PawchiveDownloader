<script lang="ts">
  import { fade } from 'svelte/transition';
  import { openUrl } from '../../lib/api';
  import type { PostFile } from '../../lib/types';
  import { t } from '../../ui/state/app.svelte';
  import Icon from '../../ui/components/Icon.svelte';
  import IconButton from '../../ui/components/IconButton.svelte';
  import Spinner from '../../ui/components/Spinner.svelte';

  interface Props {
    /** Images that can be shown, in order. */
    files: PostFile[];
    /** Index into `files` of the one shown, or null when closed. */
    index: number | null;
    selected: Set<number>;
    onindex: (index: number | null) => void;
    ontoggle: (fileIndex: number) => void;
  }

  let { files, index, selected, onindex, ontoggle }: Props = $props();

  const file = $derived(index === null ? null : (files[index] ?? null));
  // The full image when the site has it. The thumbnail shows at once and
  // the full image replaces it once it arrives; when the media server is
  // down the thumbnail simply stays, with a note saying so.
  const full = $derived(
    file && file.category === 'image' && file.url && !file.previewOnly ? file.url : null,
  );
  const thumb = $derived(file?.thumbnailUrl ?? null);
  type Load = 'loading' | 'ready' | 'failed';
  let fullState = $state<Load>('loading');
  let thumbState = $state<Load>('loading');

  $effect(() => {
    void file;
    fullState = full ? 'loading' : 'failed';
    thumbState = thumb ? 'loading' : 'failed';
  });

  const showFull = $derived(!!full && fullState === 'ready');
  const showThumb = $derived(!showFull && !!thumb && thumbState === 'ready');
  const waiting = $derived(
    !showFull && !showThumb && (fullState === 'loading' || thumbState === 'loading'),
  );
  const failed = $derived(fullState === 'failed' && thumbState === 'failed');
  /** Still fetching the full image over a preview already shown. */
  const upgrading = $derived(showThumb && !!full && fullState === 'loading');
  const thumbOnly = $derived(showThumb && !!full && fullState === 'failed');

  function step(delta: number) {
    if (index === null || files.length === 0) return;
    onindex((index + delta + files.length) % files.length);
  }

  let box = $state<HTMLDivElement>();

  function onkeydown(e: KeyboardEvent) {
    if (index === null || e.altKey || e.ctrlKey || e.metaKey) return;
    // Browse keeps pages loaded out of sight; a viewer there has no say.
    if (box && !box.checkVisibility()) return;
    if (e.key === 'Escape') onindex(null);
    else if (e.key === 'ArrowRight') step(1);
    else if (e.key === 'ArrowLeft') step(-1);
    else if (e.key === ' ' && file) {
      e.preventDefault();
      ontoggle(file.index);
    } else return;
    e.stopPropagation();
  }
</script>

<svelte:window {onkeydown} />

{#if file && (full || thumb)}
  <div
    bind:this={box}
    class="lightbox"
    role="dialog"
    aria-modal="true"
    aria-label={file.name}
    tabindex="-1"
    transition:fade={{ duration: 140 }}
  >
    <button class="scrim" aria-label={t('common.close')} onclick={() => onindex(null)}></button>
    <header>
      <span class="name truncate selectable">{file.name}</span>
      <span class="count mono-num">{(index ?? 0) + 1} / {files.length}</span>
      <button
        class="pick"
        class:on={selected.has(file.index)}
        aria-pressed={selected.has(file.index)}
        onclick={() => ontoggle(file.index)}
      >
        <Icon name={selected.has(file.index) ? 'checkCircle' : 'plus'} />
        {selected.has(file.index) ? t('lightbox.selected') : t('lightbox.select')}
      </button>
      {#if file.url ?? file.thumbnailUrl}
        <IconButton
          icon="external"
          label={t('file.open')}
          onclick={() => openUrl((file.url ?? file.thumbnailUrl) as string)}
        />
      {/if}
      <IconButton
        icon="close"
        label={t('common.close')}
        tipSide="left"
        onclick={() => onindex(null)}
      />
    </header>
    <div class="stage">
      {#if files.length > 1}
        <button class="nav prev" aria-label={t('lightbox.previous')} onclick={() => step(-1)}>
          <Icon name="chevronLeft" size={26} />
        </button>
      {/if}
      {#key file.index}
        {#if thumb && !showFull}
          <img
            src={thumb}
            alt={file.name}
            class:hidden={!showThumb}
            onload={() => (thumbState = 'ready')}
            onerror={() => (thumbState = 'failed')}
          />
        {/if}
        {#if full}
          <img
            src={full}
            alt={file.name}
            class:gone={!showFull}
            onload={() => (fullState = 'ready')}
            onerror={() => (fullState = 'failed')}
          />
        {/if}
      {/key}
      {#if waiting}<span class="state"><Spinner /></span>{/if}
      {#if failed}<span class="state faint">{t('lightbox.failed')}</span>{/if}
      {#if upgrading}
        <span class="note" transition:fade={{ duration: 140 }}>
          <Spinner size={12} />
          {t('lightbox.loadingFull')}
        </span>
      {:else if thumbOnly}
        <span class="note" transition:fade={{ duration: 140 }}>{t('lightbox.thumbOnly')}</span>
      {/if}
      {#if files.length > 1}
        <button class="nav next" aria-label={t('lightbox.next')} onclick={() => step(1)}>
          <Icon name="chevronRight" size={26} />
        </button>
      {/if}
    </div>
    <footer class="faint">{t('lightbox.keys')}</footer>
  </div>
{/if}

<style>
  .lightbox {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: grid;
    grid-template-rows: auto 1fr auto;
    color: #fff;
  }
  .scrim {
    position: absolute;
    inset: 0;
    border: 0;
    background: rgb(6 7 12 / 0.92);
    cursor: zoom-out;
  }
  header,
  .stage,
  footer {
    position: relative;
  }
  header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
  }
  .name {
    flex: 1;
    min-width: 0;
    font-weight: 600;
  }
  .count {
    font-size: var(--fs-sm);
    opacity: 0.75;
  }
  .pick {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 12px;
    border-radius: 99px;
    border: 1px solid rgb(255 255 255 / 0.25);
    background: rgb(255 255 255 / 0.08);
    color: #fff;
    font-size: var(--fs-sm);
  }
  .pick.on {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-contrast);
  }
  .stage {
    display: grid;
    place-items: center;
    min-height: 0;
    padding: 0 64px;
    pointer-events: none;
  }
  .stage > * {
    pointer-events: auto;
  }
  img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    border-radius: 6px;
    box-shadow: var(--shadow-lg);
  }
  img.hidden {
    visibility: hidden;
  }
  /* Loads out of sight until it can replace the preview. */
  img.gone {
    position: absolute;
    width: 1px;
    height: 1px;
    opacity: 0;
    pointer-events: none;
  }
  .state {
    position: absolute;
  }
  .note {
    position: absolute;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    bottom: 12px;
    padding: 4px 12px;
    border-radius: 99px;
    background: rgb(0 0 0 / 0.6);
    font-size: var(--fs-sm);
    opacity: 0.85;
  }
  .nav {
    position: absolute;
    top: 50%;
    translate: 0 -50%;
    width: 44px;
    height: 44px;
    border-radius: 99px;
    border: 0;
    display: grid;
    place-items: center;
    background: rgb(255 255 255 / 0.1);
    color: #fff;
    transition: background-color var(--dur-fast) var(--ease);
  }
  .nav:hover {
    background: rgb(255 255 255 / 0.2);
  }
  .prev {
    left: 12px;
  }
  .next {
    right: 12px;
  }
  footer {
    padding: 8px 14px 12px;
    text-align: center;
    font-size: var(--fs-xs);
    color: rgb(255 255 255 / 0.6);
  }
</style>
