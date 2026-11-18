<script lang="ts">
  import { slide } from 'svelte/transition';
  import { api } from '../../lib/api';
  import { app, fmt, t } from '../state/app.svelte';
  import { go } from '../state/router.svelte';
  import Button from './Button.svelte';
  import Icon from './Icon.svelte';

  let now = $state(Date.now());

  // A ticking clock only while a countdown runs.
  $effect(() => {
    if (!app.queue.actionAt) return;
    now = Date.now();
    const timer = setInterval(() => (now = Date.now()), 500);
    return () => clearInterval(timer);
  });

  // Only worth saying while something is queued.
  const hold = $derived(
    app.jobs.some((j) => ['queued', 'running', 'resolving'].includes(j.status))
      ? app.queue.hold
      : null,
  );
  const seconds = $derived(
    app.queue.actionAt ? Math.max(0, Math.ceil((app.queue.actionAt - now) / 1000)) : 0,
  );
  const clock = (ms: number) =>
    new Date(ms).toLocaleTimeString(app.locale.replace('_', '-'), {
      hour: '2-digit',
      minute: '2-digit',
    });
</script>

{#if app.queue.actionAt}
  <div class="banner warn" role="alert" transition:slide={{ duration: 180 }}>
    <Icon name="power" />
    <span class="text">
      {t(`whenDone.countdown.${app.queue.whenDone}`, { seconds })}
    </span>
    <Button size="sm" variant="secondary" onclick={() => api.cancelWhenDone()}>
      {t('common.cancel')}
    </Button>
  </div>
{:else if hold}
  <div class="banner" role="status" transition:slide={{ duration: 180 }}>
    {#if hold.kind === 'schedule'}
      <Icon name="clock" />
      <span class="text">{t('hold.schedule', { time: clock(hold.opensAt) })}</span>
      <Button size="sm" variant="ghost" onclick={() => go('settings', 'downloads')}>
        {t('hold.change')}
      </Button>
    {:else}
      <Icon name="harddisk" />
      <span class="text">
        {t('hold.disk', { free: fmt.bytes(hold.free), need: fmt.bytes(hold.need) })}
      </span>
      <Button size="sm" variant="ghost" onclick={() => go('settings', 'downloads')}>
        {t('hold.change')}
      </Button>
    {/if}
  </div>
{/if}

<style>
  .banner {
    position: sticky;
    top: 0;
    z-index: 20;
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0 0 12px;
    padding: 8px 12px;
    border-radius: var(--radius-md);
    border: 1px solid color-mix(in srgb, var(--info) 40%, transparent);
    background: color-mix(in srgb, var(--info) 12%, var(--bg-elev));
    font-size: var(--fs-sm);
  }
  .warn {
    border-color: color-mix(in srgb, var(--warning) 50%, transparent);
    background: color-mix(in srgb, var(--warning) 14%, var(--bg-elev));
  }
  .text {
    flex: 1;
    min-width: 0;
  }
</style>
