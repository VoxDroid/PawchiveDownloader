<script lang="ts">
  /** Full screen on and off for a viewer; F does the same. Leaves it on close. */
  import { onDestroy } from 'svelte';
  import { isFullscreen, setFullscreen } from '../../lib/fullscreen';
  import { t } from '../state/app.svelte';

  let on = $state(false);
  let entered = false;

  async function toggle() {
    const next = !(await isFullscreen().catch(() => on));
    await setFullscreen(next).catch(() => undefined);
    on = next;
    entered = next;
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== 'f' && e.key !== 'F') return;
    if (e.altKey || e.ctrlKey || e.metaKey) return;
    const target = e.target;
    if (target instanceof Element && target.closest('input, textarea, [contenteditable]')) return;
    e.preventDefault();
    void toggle();
  }

  onDestroy(() => {
    if (entered) void setFullscreen(false).catch(() => undefined);
  });
</script>

<svelte:window {onkeydown} />

<button
  class="fs"
  aria-label={on ? t('viewer.exitFullscreen') : t('viewer.fullscreen')}
  data-tip={on ? t('viewer.exitFullscreen') : t('viewer.fullscreen')}
  data-tip-side="bottom"
  onclick={toggle}
>
  <svg viewBox="0 0 24 24" aria-hidden="true">
    {#if on}
      <path d="M9 4v5H4M15 4v5h5M9 20v-5H4M15 20v-5h5" />
    {:else}
      <path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5" />
    {/if}
  </svg>
</button>

<style>
  .fs {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: inherit;
    transition: background-color var(--dur-fast) var(--ease);
  }
  .fs:hover {
    background: rgb(255 255 255 / 0.12);
  }
  svg {
    width: 18px;
    height: 18px;
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
</style>
