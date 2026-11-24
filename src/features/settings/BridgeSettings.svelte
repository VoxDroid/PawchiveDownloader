<script lang="ts">
  import { onMount } from 'svelte';
  import { fade, slide } from 'svelte/transition';
  import { api } from '../../lib/api';
  import type { BridgePairing, BridgeStatus } from '../../lib/types';
  import { app, fmt, t, updateSettings } from '../../ui/state/app.svelte';
  import { toast } from '../../ui/state/toasts.svelte';
  import Button from '../../ui/components/Button.svelte';
  import Icon from '../../ui/components/Icon.svelte';
  import IconButton from '../../ui/components/IconButton.svelte';
  import Spinner from '../../ui/components/Spinner.svelte';
  import Switch from '../../ui/components/Switch.svelte';
  import SettingRow from '../../ui/layout/SettingRow.svelte';

  /** A paired extension checks in every minute while its browser runs. */
  const CONNECTED_MS = 150_000;
  const CODE_MS = 120_000;

  let status = $state<BridgeStatus | null>(null);
  let now = $state(Date.now());
  /** The browser that paired while the code was on show, for a moment. */
  let welcomed = $state<BridgePairing | null>(null);
  let known: Set<string> | null = null;
  let waiting = false;

  async function refresh() {
    const next = await api.bridgeStatus().catch(() => null);
    if (next && known && waiting) {
      const added = next.pairings.find((p) => !known?.has(p.id));
      if (added) {
        waiting = false;
        welcomed = added;
        toast(t('bridge.pairedToast', { browser: browserOf(added).name }), 'success');
        if (next.code) void api.bridgeCancelPairing();
        next.code = null;
      }
    }
    if (next) known = new Set(next.pairings.map((p) => p.id));
    status = next;
  }

  onMount(() => {
    void refresh();
    // Cheap local calls: keeps the countdown, the connected lights and a new
    // pairing current while this page is open.
    const timer = setInterval(() => {
      now = Date.now();
      void refresh();
    }, 1000);
    return () => clearInterval(timer);
  });

  const secondsLeft = $derived(
    status?.codeExpiresAt ? Math.max(0, Math.ceil((status.codeExpiresAt - now) / 1000)) : 0,
  );
  const showingCode = $derived(!!status?.code && secondsLeft > 0);
  const pairings = $derived(
    [...(status?.pairings ?? [])].sort(
      (a, b) => (b.lastUsedAt ?? b.createdAt) - (a.lastUsedAt ?? a.createdAt),
    ),
  );
  const isConnected = (p: BridgePairing) => !!p.lastUsedAt && now - p.lastUsedAt < CONNECTED_MS;
  const connected = $derived(pairings.filter(isConnected));

  const BROWSERS: Record<string, string> = {
    chrome: 'Google Chrome',
    edge: 'Microsoft Edge',
    firefox: 'Mozilla Firefox',
    brave: 'Brave',
    opera: 'Opera',
    vivaldi: 'Vivaldi',
  };

  /** "Edge: Pawchive Downloader", as the extension names itself when pairing. */
  function browserOf(p: BridgePairing): { name: string; extension: string } {
    const [head, ...rest] = p.name.split(':');
    const browser = BROWSERS[(head ?? '').trim().toLowerCase()];
    if (browser && rest.length) return { name: browser, extension: rest.join(':').trim() };
    return { name: BROWSERS[p.name.trim().toLowerCase()] ?? p.name, extension: '' };
  }

  const headline = $derived.by(() => {
    if (!status?.running) return t('bridge.starting');
    if (connected.length === 1)
      return t('bridge.connectedTo', { browser: browserOf(connected[0] as BridgePairing).name });
    if (connected.length > 1) return t('bridge.connectedMany', { count: connected.length });
    if (pairings.length) return t('bridge.pairedIdle', { count: pairings.length });
    return t('bridge.noneYet');
  });
  const tone = $derived(
    !status?.running ? 'off' : connected.length ? 'on' : pairings.length ? 'idle' : 'none',
  );

  async function pair() {
    welcomed = null;
    known = new Set(status?.pairings.map((p) => p.id) ?? []);
    waiting = true;
    status = await api.bridgePair();
  }

  async function cancel() {
    waiting = false;
    status = await api.bridgeCancelPairing();
  }

  async function revoke(p: BridgePairing) {
    if (welcomed?.id === p.id) welcomed = null;
    status = await api.bridgeRevoke(p.id);
  }

  async function copyCode() {
    if (!status?.code) return;
    try {
      await navigator.clipboard.writeText(status.code);
      toast(t('common.copied'), 'success');
    } catch {
      // The clipboard may be unavailable; the code is on screen anyway.
    }
  }

  const clock = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
</script>

<SettingRow label={t('bridge.enable')} description={t('bridge.enableHint')}>
  <Switch
    label={t('bridge.enable')}
    checked={app.settings.bridgeEnabled}
    onchange={(v) => updateSettings({ bridgeEnabled: v })}
  />
</SettingRow>

{#if app.settings.bridgeEnabled}
  <div class="bridge" transition:slide={{ duration: 180 }}>
    <div class="head {tone}">
      <span class="pulse" aria-hidden="true"></span>
      <div class="what" aria-live="polite">
        <strong>{headline}</strong>
        <span class="faint">
          {#if status?.running}
            {t('bridge.listening', { address: `127.0.0.1:${status.port}` })}
          {:else}
            {t('bridge.startingHint')}
          {/if}
        </span>
      </div>
      {#if !showingCode}
        <Button
          size="sm"
          variant={pairings.length ? 'secondary' : 'primary'}
          icon="link"
          disabled={!status?.running}
          onclick={pair}
        >
          {pairings.length ? t('bridge.pairAnother') : t('bridge.pairBrowser')}
        </Button>
      {/if}
    </div>

    {#if showingCode && status?.code}
      <div class="pairing" transition:slide={{ duration: 200 }}>
        <ol class="steps">
          <li>
            <span class="num">1</span>
            <div>
              <strong>{t('bridge.step1')}</strong>
              <span>{t('bridge.step1Hint')}</span>
            </div>
          </li>
          <li>
            <span class="num">2</span>
            <div>
              <strong>{t('bridge.step2')}</strong>
              <div class="code-row">
                <span class="digits mono-num" aria-label={status.code.split('').join(' ')}>
                  {#each status.code.split('') as digit, i (i)}
                    <span class="digit">{digit}</span>
                  {/each}
                </span>
                <IconButton icon="copy" label={t('common.copy')} onclick={copyCode} />
              </div>
              <div class="timer" class:low={secondsLeft <= 20}>
                <span class="track">
                  <span
                    class="fill"
                    style:transform="scaleX({Math.min(1, (secondsLeft * 1000) / CODE_MS)})"
                  ></span>
                </span>
                <span class="mono-num">{t('bridge.expiresIn', { time: clock(secondsLeft) })}</span>
              </div>
            </div>
          </li>
          <li>
            <span class="num wait"><Spinner size={14} /></span>
            <div>
              <strong>{t('bridge.step3')}</strong>
              <span>{t('bridge.step3Hint')}</span>
            </div>
          </li>
        </ol>
        <div class="actions">
          <Button size="sm" variant="ghost" onclick={cancel}>{t('common.cancel')}</Button>
          <Button size="sm" variant="secondary" icon="retry" onclick={pair}>
            {t('bridge.newCode')}
          </Button>
        </div>
      </div>
    {:else if welcomed}
      <div class="welcome" transition:slide={{ duration: 200 }}>
        <Icon name="checkCircle" size={22} />
        <div>
          <strong>{t('bridge.pairedToast', { browser: browserOf(welcomed).name })}</strong>
          <span>{t('bridge.pairedBody')}</span>
        </div>
        <IconButton icon="close" label={t('common.close')} onclick={() => (welcomed = null)} />
      </div>
    {/if}

    {#if pairings.length}
      <ul class="browsers" aria-label={t('bridge.pairedBrowsers')}>
        {#each pairings as p (p.id)}
          {@const b = browserOf(p)}
          {@const live = isConnected(p)}
          <li class:live in:fade={{ duration: 160 }}>
            <span class="avatar"><Icon name="web" size={20} /><span class="dot"></span></span>
            <div class="who">
              <strong class="truncate">{b.name}</strong>
              <span class="faint truncate">
                {b.extension ? `${b.extension} · ` : ''}{t('bridge.paired', {
                  when: fmt.relative(p.createdAt),
                })}
              </span>
            </div>
            <span class="presence" class:on={live}>
              {#if live}
                {t('bridge.connected')}
              {:else if p.lastUsedAt}
                {t('bridge.lastSeen', { when: fmt.relative(p.lastUsedAt) })}
              {:else}
                {t('bridge.neverUsed')}
              {/if}
            </span>
            <IconButton
              icon="trash"
              label={t('bridge.revoke')}
              tipSide="left"
              onclick={() => revoke(p)}
            />
          </li>
        {/each}
      </ul>
    {/if}
  </div>
{/if}

<style>
  .bridge {
    display: grid;
    gap: 12px;
    padding: 14px 18px 16px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 14px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: var(--bg-elev-2);
  }
  .head.on {
    border-color: color-mix(in srgb, var(--success) 45%, var(--border));
    background: color-mix(in srgb, var(--success) 8%, var(--bg-elev-2));
  }
  .what {
    display: grid;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }
  .what span {
    font-size: var(--fs-sm);
  }
  /* A status light: green and breathing while a browser is connected. */
  .pulse {
    position: relative;
    flex: none;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--text-faint);
  }
  .head.idle .pulse {
    background: var(--warning);
  }
  .head.on .pulse {
    background: var(--success);
  }
  .head.on .pulse::after {
    content: '';
    position: absolute;
    inset: -4px;
    border-radius: 50%;
    border: 2px solid var(--success);
    animation: breathe 2s var(--ease) infinite;
  }
  @keyframes breathe {
    from {
      opacity: 0.7;
      transform: scale(0.6);
    }
    to {
      opacity: 0;
      transform: scale(1.5);
    }
  }
  .pairing {
    display: grid;
    gap: 14px;
    padding: 16px;
    border-radius: var(--radius-md);
    border: 1px solid color-mix(in srgb, var(--accent) 45%, var(--border));
    background: var(--accent-softer);
  }
  .steps {
    display: grid;
    gap: 14px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .steps li {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }
  .steps li > div {
    display: grid;
    gap: 4px;
    min-width: 0;
    flex: 1;
  }
  .steps li > div > span {
    color: var(--text-dim);
    font-size: var(--fs-sm);
  }
  .num {
    flex: none;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-contrast);
    font-size: var(--fs-xs);
    font-weight: 700;
  }
  .num.wait {
    background: var(--bg-elev-3);
    color: var(--accent-strong);
  }
  .code-row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 4px;
  }
  .digits {
    display: flex;
    gap: 6px;
  }
  .digit {
    display: grid;
    place-items: center;
    width: 38px;
    height: 46px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-strong);
    background: var(--bg-elev);
    font-size: var(--fs-xl);
    font-weight: 700;
    color: var(--text);
  }
  .digit:nth-child(3) {
    margin-right: 8px;
  }
  .timer {
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: 300px;
    margin-top: 6px;
    font-size: var(--fs-xs);
    color: var(--text-dim);
  }
  .track {
    flex: 1;
    height: 4px;
    border-radius: 99px;
    background: var(--bg-elev-3);
    overflow: hidden;
  }
  .fill {
    display: block;
    height: 100%;
    background: var(--accent);
    transform-origin: left;
    transition: transform 1s linear;
  }
  .timer.low .fill {
    background: var(--warning);
  }
  .timer.low {
    color: var(--warning);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .welcome {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    border-radius: var(--radius-md);
    border: 1px solid color-mix(in srgb, var(--success) 45%, var(--border));
    background: color-mix(in srgb, var(--success) 10%, var(--bg-elev));
  }
  .welcome > :global(.icon) {
    color: var(--success);
    flex: none;
  }
  .welcome div {
    display: grid;
    gap: 2px;
    flex: 1;
  }
  .welcome span {
    color: var(--text-dim);
    font-size: var(--fs-sm);
  }
  .browsers {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .browsers li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 10px 10px 12px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: var(--bg-elev);
  }
  .avatar {
    position: relative;
    flex: none;
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: 50%;
    background: var(--bg-elev-3);
    color: var(--text-dim);
  }
  .avatar .dot {
    position: absolute;
    right: -1px;
    bottom: -1px;
    width: 11px;
    height: 11px;
    border-radius: 50%;
    border: 2px solid var(--bg-elev);
    background: var(--text-faint);
  }
  li.live .avatar {
    color: var(--success);
  }
  li.live .avatar .dot {
    background: var(--success);
  }
  .who {
    display: grid;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }
  .who span {
    font-size: var(--fs-sm);
  }
  .presence {
    flex: none;
    padding: 3px 10px;
    border-radius: 99px;
    background: var(--bg-elev-2);
    color: var(--text-dim);
    font-size: var(--fs-xs);
    font-weight: 600;
  }
  .presence.on {
    background: color-mix(in srgb, var(--success) 16%, transparent);
    color: var(--success);
  }
</style>
