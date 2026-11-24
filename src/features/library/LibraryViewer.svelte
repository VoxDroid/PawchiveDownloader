<script module lang="ts">
  import type { MediaFile, MediaPost } from '../../lib/types';
  export interface ViewerItem {
    file: MediaFile;
    post: MediaPost;
  }
</script>

<script lang="ts">
  import { fade } from 'svelte/transition';
  import { api } from '../../lib/api';
  import { mediaSrc } from '../../lib/media';
  import { pageUrls } from '../../lib/urls';
  import { fmt, t } from '../../ui/state/app.svelte';
  import { describeError } from '../../ui/state/errors';
  import { openLink } from '../../ui/state/router.svelte';
  import { toast } from '../../ui/state/toasts.svelte';
  import Icon from '../../ui/components/Icon.svelte';
  import IconButton from '../../ui/components/IconButton.svelte';
  import Spinner from '../../ui/components/Spinner.svelte';
  import ZoomImage from '../../ui/components/ZoomImage.svelte';
  import FullscreenButton from '../../ui/components/FullscreenButton.svelte';

  interface Props {
    items: ViewerItem[];
    /** Index into `items` of the one shown, or null when closed. */
    index: number | null;
    onindex: (index: number | null) => void;
  }

  let { items, index, onindex }: Props = $props();

  const SLIDE_MS = 4000;
  const item = $derived(index === null ? null : (items[index] ?? null));
  const file = $derived(item?.file ?? null);
  const src = $derived(file ? mediaSrc(file) : '');
  let loading = $state(true);
  let failed = $state(false);
  let playing = $state(false);
  let box = $state<HTMLDivElement>();
  let video = $state<HTMLVideoElement>();

  $effect(() => {
    void src;
    loading = true;
    failed = false;
  });

  /** The files of the post on show, for the strip at the bottom. */
  const strip = $derived(item ? items.filter((i) => i.post.key === item.post.key) : []);

  function step(delta: number) {
    if (index === null || items.length === 0) return;
    onindex((index + delta + items.length) % items.length);
  }
  const close = () => {
    playing = false;
    onindex(null);
  };

  // The images on both sides are fetched and decoded ahead, so moving on is
  // instant either way.
  $effect(() => {
    if (index === null || items.length < 2) return;
    for (const delta of [1, -1]) {
      const near = items[(index + delta + items.length) % items.length]?.file;
      if (near?.kind !== 'image') continue;
      const img = new Image();
      img.src = mediaSrc(near);
      void img.decode().catch(() => undefined);
    }
  });

  // Slideshow: images move on by themselves; videos play to their end.
  $effect(() => {
    if (!playing || !file || file.kind === 'video' || loading) return;
    const timer = setTimeout(() => step(1), SLIDE_MS);
    return () => clearTimeout(timer);
  });

  function onkeydown(e: KeyboardEvent) {
    if (index === null || e.altKey || e.ctrlKey || e.metaKey) return;
    if (box && !box.checkVisibility()) return;
    if (e.key === 'Escape') close();
    else if (e.key === 'ArrowRight') step(1);
    else if (e.key === 'ArrowLeft') step(-1);
    else if (e.key === ' ') {
      if (file?.kind === 'video' && video) {
        if (video.paused) void video.play();
        else video.pause();
      } else playing = !playing;
    } else return;
    e.preventDefault();
    e.stopPropagation();
  }

  async function run(action: () => Promise<unknown>) {
    try {
      await action();
    } catch (e) {
      toast(describeError(e), 'error');
    }
  }
</script>

<svelte:window {onkeydown} />

{#if item && file}
  <div
    bind:this={box}
    class="viewer"
    role="dialog"
    aria-modal="true"
    aria-label={file.name}
    tabindex="-1"
    transition:fade={{ duration: 140 }}
  >
    <button class="scrim" aria-label={t('common.close')} onclick={close}></button>
    <header>
      <div class="what">
        <span class="name truncate selectable">{file.name}</span>
        <span class="where truncate">
          {item.post.title} · {item.post.creatorName} · {fmt.bytes(file.size)}
        </span>
      </div>
      <span class="count mono-num">{(index ?? 0) + 1} / {items.length}</span>
      {#if file.kind === 'image'}
        <IconButton
          icon={playing ? 'pause' : 'play'}
          label={playing ? t('library.stopSlideshow') : t('library.slideshow')}
          onclick={() => (playing = !playing)}
        />
      {/if}
      <IconButton
        icon="compass"
        label={t('common.openInBrowse')}
        onclick={() => {
          const url = pageUrls.post(item.post.service, item.post.creatorId, item.post.postId);
          close();
          openLink(url);
        }}
      />
      <IconButton
        icon="folderOpen"
        label={t('library.showFolder')}
        onclick={() => run(() => api.revealPath(file.path))}
      />
      <IconButton
        icon="external"
        label={t('library.openWith')}
        onclick={() => run(() => api.openMedia(file.hash))}
      />
      <FullscreenButton />
      <IconButton icon="close" label={t('common.close')} tipSide="left" onclick={close} />
    </header>

    <div class="stage">
      {#if items.length > 1}
        <button class="nav prev" aria-label={t('lightbox.previous')} onclick={() => step(-1)}>
          <Icon name="chevronLeft" size={26} />
        </button>
      {/if}
      {#key src}
        {#if file.kind === 'image'}
          {#if loading}
            <!-- The cached thumbnail at once, until the full image is ready. -->
            <img class="instant" src={mediaSrc(file, true)} alt="" aria-hidden="true" />
          {/if}
          <ZoomImage
            {src}
            alt={file.name}
            onload={() => (loading = false)}
            onerror={() => {
              loading = false;
              failed = true;
            }}
          />
        {:else}
          <!-- svelte-ignore a11y_media_has_caption -->
          <video
            bind:this={video}
            {src}
            controls
            autoplay
            playsinline
            class:hidden={loading}
            onloadeddata={() => (loading = false)}
            onended={() => playing && step(1)}
            onerror={() => {
              loading = false;
              failed = true;
            }}
          ></video>
        {/if}
      {/key}
      {#if loading && file.kind === 'video'}<span class="state"><Spinner /></span>{/if}
      {#if failed}
        <span class="state failed">
          {file.kind === 'video' ? t('library.videoFailed') : t('lightbox.failed')}
          <button class="open" onclick={() => run(() => api.openMedia(file.hash))}>
            <Icon name="external" />{t('library.openWith')}
          </button>
        </span>
      {/if}
      {#if items.length > 1}
        <button class="nav next" aria-label={t('lightbox.next')} onclick={() => step(1)}>
          <Icon name="chevronRight" size={26} />
        </button>
      {/if}
    </div>

    {#if strip.length > 1}
      <div class="strip" role="group" aria-label={item.post.title}>
        {#each strip as s (s.file.hash)}
          <button
            class="thumb"
            class:on={s.file.hash === file.hash}
            aria-label={s.file.name}
            onclick={() => onindex(items.indexOf(s))}
          >
            {#if s.file.kind === 'image'}
              <img src={mediaSrc(s.file, true)} alt="" loading="lazy" />
            {:else}
              <Icon name="video" />
            {/if}
          </button>
        {/each}
      </div>
    {/if}
    <footer class="faint">{t('library.keys')}</footer>
  </div>
{/if}

<style>
  .viewer {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: grid;
    grid-template-rows: auto 1fr auto auto;
    color: #fff;
  }
  .scrim {
    position: absolute;
    inset: 0;
    border: 0;
    background: rgb(6 7 12 / 0.94);
    cursor: zoom-out;
  }
  header,
  .stage,
  .strip,
  footer {
    position: relative;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
  }
  .what {
    display: grid;
    gap: 1px;
    flex: 1;
    min-width: 0;
  }
  .name {
    font-weight: 600;
  }
  .where {
    font-size: var(--fs-xs);
    opacity: 0.7;
  }
  .count {
    font-size: var(--fs-sm);
    opacity: 0.75;
    margin-right: 6px;
  }
  .stage {
    display: grid;
    /* One cell as big as the space left, so the picture can fit inside it. */
    grid-template: minmax(0, 1fr) / minmax(0, 1fr);
    place-items: center;
    min-height: 0;
    padding: 0 64px;
    pointer-events: none;
  }
  .stage > * {
    pointer-events: auto;
  }
  video {
    max-width: 100%;
    max-height: 100%;
    border-radius: 6px;
    background: #000;
    box-shadow: var(--shadow-lg);
  }
  .hidden {
    visibility: hidden;
  }
  .instant {
    position: absolute;
    inset: 0 64px;
    width: calc(100% - 128px);
    height: 100%;
    object-fit: contain;
    filter: blur(1.5px);
    opacity: 0.9;
    pointer-events: none;
  }
  .state {
    position: absolute;
  }
  .failed {
    display: grid;
    justify-items: center;
    gap: 12px;
    opacity: 0.85;
  }
  .open {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 12px;
    border-radius: 99px;
    border: 1px solid rgb(255 255 255 / 0.3);
    background: rgb(255 255 255 / 0.08);
    color: #fff;
  }
  .nav {
    position: absolute;
    top: 50%;
    z-index: 1;
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    margin-top: -22px;
    border-radius: 50%;
    border: 0;
    background: rgb(255 255 255 / 0.08);
    color: #fff;
  }
  .nav:hover {
    background: rgb(255 255 255 / 0.18);
  }
  .prev {
    left: 12px;
  }
  .next {
    right: 12px;
  }
  .strip {
    display: flex;
    justify-content: center;
    gap: 6px;
    padding: 10px 14px 0;
    overflow-x: auto;
  }
  .thumb {
    flex: none;
    display: grid;
    place-items: center;
    width: 52px;
    height: 52px;
    padding: 0;
    overflow: hidden;
    border-radius: 6px;
    border: 2px solid transparent;
    background: rgb(255 255 255 / 0.08);
    color: #fff;
    opacity: 0.6;
    transition:
      opacity var(--dur-fast) var(--ease),
      border-color var(--dur-fast) var(--ease);
  }
  .thumb:hover {
    opacity: 1;
  }
  .thumb.on {
    opacity: 1;
    border-color: var(--accent);
  }
  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  footer {
    padding: 8px 14px 12px;
    text-align: center;
    font-size: var(--fs-xs);
  }
</style>
