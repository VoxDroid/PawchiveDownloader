<script lang="ts">
  /**
   * A video tile: its first frame once it comes near the screen (so a big
   * library does not open every video at once), playing muted while the
   * pointer is on it.
   */
  import Icon from '../../ui/components/Icon.svelte';

  let { src }: { src: string } = $props();

  let box = $state<HTMLElement>();
  let video = $state<HTMLVideoElement>();
  let near = $state(false);
  let ready = $state(false);

  $effect(() => {
    const el = box;
    if (!el || !src) return;
    const io = new IntersectionObserver(
      (entries) => {
        const visible = entries.some((e) => e.isIntersecting);
        if (visible) near = true;
        else video?.pause();
      },
      { rootMargin: '300px' },
    );
    io.observe(el);
    return () => io.disconnect();
  });
</script>

<span class="frame" bind:this={box}>
  <span class="placeholder"><Icon name="video" size={28} /></span>
  {#if near}
    <video
      bind:this={video}
      class:ready
      src="{src}#t=0.1"
      muted
      loop
      playsinline
      preload="metadata"
      onloadeddata={() => (ready = true)}
      onpointerenter={() => void video?.play().catch(() => undefined)}
      onpointerleave={() => video?.pause()}
    ></video>
  {/if}
</span>

<style>
  .frame,
  .placeholder,
  video {
    position: absolute;
    inset: 0;
  }
  .placeholder {
    display: grid;
    place-items: center;
    color: var(--text-faint);
  }
  video {
    width: 100%;
    height: 100%;
    object-fit: cover;
    opacity: 0;
    transition: opacity var(--dur) var(--ease);
  }
  video.ready {
    opacity: 1;
  }
</style>
