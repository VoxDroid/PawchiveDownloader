<script lang="ts">
  /**
   * An image that fits its box and can be zoomed and moved: the wheel zooms
   * toward the pointer, dragging moves a zoomed image, a double click
   * switches between fitting and actual size, and + - 0 1 on the keyboard
   * do the same as the bar at the bottom.
   */
  import { t } from '../state/app.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    src: string;
    alt: string;
    /** Listen to + - 0 1 (only while this image is on show). */
    keys?: boolean;
    onload?: () => void;
    onerror?: () => void;
  }

  let { src, alt, keys = true, onload, onerror }: Props = $props();

  const MAX = 10;
  let box = $state<HTMLDivElement>();
  let img = $state<HTMLImageElement>();
  let loaded = $state(false);
  let zoom = $state(1);
  let x = $state(0);
  let y = $state(0);
  let dragging = $state(false);
  let drag: { px: number; py: number; x: number; y: number; moved: boolean } | null = null;

  $effect(() => {
    void src;
    loaded = false;
    reset();
  });

  function reset() {
    zoom = 1;
    x = 0;
    y = 0;
  }

  /** How much bigger than fitted the actual pixels are (1 when it fits as is). */
  function actualScale(): number {
    if (!img || !img.naturalWidth) return 1;
    const shown = img.getBoundingClientRect().width / zoom;
    return shown > 0 ? img.naturalWidth / shown : 1;
  }

  /** Keeps a zoomed image covering the box instead of drifting off. */
  function clamp() {
    if (!img || !box) return;
    const b = box.getBoundingClientRect();
    const w = (img.getBoundingClientRect().width / zoom) * zoom;
    const h = (img.getBoundingClientRect().height / zoom) * zoom;
    const maxX = Math.max(0, (w - b.width) / 2);
    const maxY = Math.max(0, (h - b.height) / 2);
    x = Math.min(maxX, Math.max(-maxX, x));
    y = Math.min(maxY, Math.max(-maxY, y));
  }

  /** Zooms to `next`, keeping the point under (cx, cy) where it is. */
  function zoomTo(next: number, cx?: number, cy?: number) {
    if (!box) return;
    const target = Math.min(MAX, Math.max(1, next));
    const b = box.getBoundingClientRect();
    const px = (cx ?? b.left + b.width / 2) - (b.left + b.width / 2);
    const py = (cy ?? b.top + b.height / 2) - (b.top + b.height / 2);
    const ratio = target / zoom;
    x = px - (px - x) * ratio;
    y = py - (py - y) * ratio;
    zoom = target;
    if (zoom === 1) {
      x = 0;
      y = 0;
    }
    requestAnimationFrame(clamp);
  }

  function onwheel(e: WheelEvent) {
    if (!loaded) return;
    e.preventDefault();
    zoomTo(zoom * Math.pow(1.0015, -e.deltaY), e.clientX, e.clientY);
  }

  function onpointerdown(e: PointerEvent) {
    if (e.button !== 0 || zoom <= 1) return;
    drag = { px: e.clientX, py: e.clientY, x, y, moved: false };
    dragging = true;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  }
  function onpointermove(e: PointerEvent) {
    if (!drag) return;
    x = drag.x + e.clientX - drag.px;
    y = drag.y + e.clientY - drag.py;
    drag.moved ||= Math.abs(e.clientX - drag.px) + Math.abs(e.clientY - drag.py) > 3;
    clamp();
  }
  function onpointerup() {
    drag = null;
    dragging = false;
  }

  function ondblclick(e: MouseEvent) {
    if (zoom > 1) zoomTo(1);
    else zoomTo(Math.max(2, actualScale()), e.clientX, e.clientY);
  }

  function onkeydown(e: KeyboardEvent) {
    if (!keys || !loaded || e.altKey || e.ctrlKey || e.metaKey) return;
    if (box && !box.checkVisibility()) return;
    if (e.key === '+' || e.key === '=') zoomTo(zoom * 1.4);
    else if (e.key === '-') zoomTo(zoom / 1.4);
    else if (e.key === '0') zoomTo(1);
    else if (e.key === '1') zoomTo(actualScale());
    else return;
    e.preventDefault();
  }

  const percent = $derived(Math.round(zoom * 100));
</script>

<svelte:window {onkeydown} />

<div
  bind:this={box}
  class="zoom"
  class:zoomed={zoom > 1}
  class:dragging
  role="img"
  aria-label={alt}
  {onwheel}
  {onpointerdown}
  {onpointermove}
  {onpointerup}
  onpointercancel={onpointerup}
  {ondblclick}
>
  <img
    bind:this={img}
    {src}
    {alt}
    class:hidden={!loaded}
    draggable="false"
    style:transform="translate({x}px, {y}px) scale({zoom})"
    onload={() => {
      loaded = true;
      onload?.();
    }}
    onerror={() => onerror?.()}
  />
</div>

{#if loaded}
  <div class="bar" role="toolbar" aria-label={t('zoom.label')}>
    <button
      aria-label={t('zoom.out')}
      data-tip={t('zoom.out')}
      disabled={zoom <= 1}
      onclick={() => zoomTo(zoom / 1.4)}
    >
      <Icon name="minus" />
    </button>
    <button class="pct mono-num" data-tip={t('zoom.fit')} onclick={() => zoomTo(1)}>
      {percent}%
    </button>
    <button
      aria-label={t('zoom.in')}
      data-tip={t('zoom.in')}
      disabled={zoom >= MAX}
      onclick={() => zoomTo(zoom * 1.4)}
    >
      <Icon name="plus" />
    </button>
    <span class="sep"></span>
    <button class="word" data-tip={t('zoom.fitHint')} onclick={() => zoomTo(1)}>
      {t('zoom.fitShort')}
    </button>
    <button class="word" data-tip={t('zoom.actualHint')} onclick={() => zoomTo(actualScale())}>
      1:1
    </button>
  </div>
{/if}

<style>
  .zoom {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    touch-action: none;
    cursor: zoom-in;
    /* Clicks beside the picture reach the backdrop, which closes the viewer. */
    pointer-events: none;
  }
  .zoom.zoomed {
    cursor: grab;
  }
  .zoom.dragging {
    cursor: grabbing;
  }
  img {
    pointer-events: auto;
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    border-radius: 6px;
    box-shadow: var(--shadow-lg);
    user-select: none;
    transform-origin: center;
    transition: transform 120ms var(--ease);
    will-change: transform;
  }
  .dragging img {
    transition: none;
  }
  img.hidden {
    visibility: hidden;
  }
  .bar {
    position: absolute;
    left: 50%;
    bottom: 10px;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px;
    border-radius: 99px;
    transform: translateX(-50%);
    background: rgb(12 14 20 / 0.78);
    border: 1px solid rgb(255 255 255 / 0.14);
    backdrop-filter: blur(8px);
    color: #fff;
    /* Viewers turn pointer events off around the picture; the bar needs them. */
    pointer-events: auto;
  }
  .bar button {
    display: grid;
    place-items: center;
    min-width: 30px;
    height: 28px;
    padding: 0 8px;
    border: 0;
    border-radius: 99px;
    background: none;
    color: inherit;
    font: inherit;
    font-size: var(--fs-xs);
    font-weight: 600;
  }
  .bar button:hover:not(:disabled) {
    background: rgb(255 255 255 / 0.12);
  }
  .bar button:disabled {
    opacity: 0.4;
  }
  .pct {
    min-width: 52px !important;
  }
  .sep {
    width: 1px;
    height: 16px;
    margin: 0 4px;
    background: rgb(255 255 255 / 0.2);
  }
</style>
