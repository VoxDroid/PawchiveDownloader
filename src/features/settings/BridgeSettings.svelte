<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../../lib/api';
  import type { BridgeStatus } from '../../lib/types';
  import { app, fmt, t, updateSettings } from '../../ui/state/app.svelte';
  import Button from '../../ui/components/Button.svelte';
  import IconButton from '../../ui/components/IconButton.svelte';
  import Switch from '../../ui/components/Switch.svelte';
  import SettingRow from '../../ui/layout/SettingRow.svelte';

  let status = $state<BridgeStatus | null>(null);
  let now = $state(Date.now());

  async function refresh() {
    status = await api.bridgeStatus().catch(() => null);
  }

  onMount(() => {
    void refresh();
    // The server follows the setting within a few seconds; keep the line fresh.
    const timer = setInterval(() => {
      now = Date.now();
      if (!status?.code || now > (status.codeExpiresAt ?? 0)) void refresh();
    }, 1000);
    return () => clearInterval(timer);
  });

  const secondsLeft = $derived(
    status?.codeExpiresAt ? Math.max(0, Math.ceil((status.codeExpiresAt - now) / 1000)) : 0,
  );

  async function pair() {
    status = await api.bridgePair();
  }

  async function revoke(id: string) {
    status = await api.bridgeRevoke(id);
  }
</script>

<SettingRow label={t('bridge.enable')} description={t('bridge.enableHint')}>
  <Switch
    label={t('bridge.enable')}
    checked={app.settings.bridgeEnabled}
    onchange={(v) => updateSettings({ bridgeEnabled: v })}
  />
</SettingRow>
{#if app.settings.bridgeEnabled}
  <SettingRow
    label={t('bridge.status')}
    description={status?.running
      ? t('bridge.listening', { address: `127.0.0.1:${status.port}` })
      : t('bridge.starting')}
  >
    {#if status?.code && secondsLeft > 0}
      <span class="code mono-num" aria-live="polite">{status.code}</span>
      <span class="faint mono-num">{secondsLeft} s</span>
    {:else}
      <Button size="sm" icon="link" disabled={!status?.running} onclick={pair}>
        {t('bridge.pair')}
      </Button>
    {/if}
  </SettingRow>
  {#if status?.code && secondsLeft > 0}
    <p class="note">{t('bridge.codeHint')}</p>
  {/if}
  {#each status?.pairings ?? [] as p (p.id)}
    <SettingRow
      label={p.name}
      description={p.lastUsedAt
        ? t('bridge.lastUsed', { when: fmt.relative(p.lastUsedAt) })
        : t('bridge.paired', { when: fmt.relative(p.createdAt) })}
    >
      <IconButton icon="trash" label={t('bridge.revoke')} onclick={() => revoke(p.id)} />
    </SettingRow>
  {/each}
{/if}

<style>
  .code {
    font-size: var(--fs-xl);
    font-weight: 700;
    letter-spacing: 0.2em;
  }
  .note {
    margin: 0;
    padding: 0 18px 14px;
    font-size: var(--fs-sm);
    color: var(--text-dim);
  }
</style>
