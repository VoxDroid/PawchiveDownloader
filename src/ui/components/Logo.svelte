<script lang="ts">
  let { size = 28 }: { size?: number } = $props();

  // The hand-drawn icon. Small logos use the bolder small-size drawing.
  const src = $derived(size <= 40 ? '/icon/48.png' : '/icon/128.png');
  const srcset = $derived(size <= 40 ? '/icon/48.png 1x, /icon/128.png 2x' : undefined);
</script>

<img class="logo" {src} {srcset} width={size} height={size} alt="" draggable="false" />

<style>
  .logo {
    flex: none;
    display: block;
    object-fit: contain;
    user-select: none;
    filter: drop-shadow(0 1px 1px rgb(0 0 0 / 0.18));
  }
</style>
