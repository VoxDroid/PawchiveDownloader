<script lang="ts">
  import { onMount } from 'svelte';
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import {
    app,
    onOpenLinks,
    overallProgress,
    startApp,
    t,
    themeDocument,
  } from './ui/state/app.svelte';
  import { ROUTES, go, openLink, router, type Route } from './ui/state/router.svelte';
  import Icon from './ui/components/Icon.svelte';
  import Logo from './ui/components/Logo.svelte';
  import Spinner from './ui/components/Spinner.svelte';
  import Toaster from './ui/components/Toaster.svelte';
  import SupportCard from './ui/components/SupportCard.svelte';
  import BrowsePage from './features/browse/BrowsePage.svelte';
  import BatchDialog from './features/browse/BatchDialog.svelte';
  import DownloadsPage from './features/downloads/DownloadsPage.svelte';
  import HistoryPage from './features/history/HistoryPage.svelte';
  import WatchlistPage from './features/watchlist/WatchlistPage.svelte';
  import FavoritesPage from './features/favorites/FavoritesPage.svelte';
  import LogsPage from './features/logs/LogsPage.svelte';
  import SettingsPage from './features/settings/SettingsPage.svelte';
  import HelpPage from './features/help/HelpPage.svelte';
  import AboutPage from './features/about/AboutPage.svelte';
  import { batch } from './features/browse/batch.svelte';

  themeDocument();

  const progress = $derived(overallProgress(app.jobs));
  let failedToStart = $state<string | null>(null);
  let dragging = $state(false);

  const GROUPS = ['main', 'library', 'app'] as const;

  onMount(() => {
    startApp()
      .then(() => {
        const links = app.info?.args.filter((a) => a.includes('pawchive.pw')) ?? [];
        if (links.length === 1) openLink(links[0] as string);
        else if (links.length > 1) batch.show(links.join('\n'));
      })
      .catch((e) => (failedToStart = String(e)));
    const off = onOpenLinks((text) => {
      const lines = text.split(/\s+/).filter(Boolean);
      if (lines.length === 1) openLink(lines[0] as string);
      else batch.show(text);
    });
    return off;
  });

  $effect(() => {
    document.title = `${t(`nav.${router.route}`)} - ${t('app.name')}`;
  });

  function onkeydown(event: KeyboardEvent) {
    const mod = event.ctrlKey || event.metaKey;
    if (!mod || event.altKey) return;
    const n = Number(event.key);
    if (n >= 1 && n <= ROUTES.length) {
      event.preventDefault();
      go((ROUTES[n - 1] as (typeof ROUTES)[number]).id);
    } else if (event.key === ',') {
      event.preventDefault();
      go('settings');
    } else if (event.key.toLowerCase() === 'l') {
      event.preventDefault();
      go('browse');
      requestAnimationFrame(() => window.dispatchEvent(new CustomEvent('pdl:focus-link')));
    } else if (event.key.toLowerCase() === 'b' && event.shiftKey) {
      event.preventDefault();
      batch.show('');
    }
  }

  // Links dragged from a browser onto the window.
  function ondragover(event: DragEvent) {
    if (
      !event.dataTransfer?.types.some(
        (t) => t === 'text/uri-list' || t === 'text/plain' || t === 'Files',
      )
    )
      return;
    event.preventDefault();
    dragging = true;
  }

  async function ondrop(event: DragEvent) {
    event.preventDefault();
    dragging = false;
    const data = event.dataTransfer;
    if (!data) return;
    let text = data.getData('text/uri-list') || data.getData('text/plain');
    if (!text && data.files.length) {
      const file = data.files[0] as File;
      if (file.size < 4 * 1024 * 1024) text = await file.text();
    }
    if (!text) return;
    const links = text.split(/\s+/).filter((l) => l.includes('pawchive.pw'));
    if (links.length === 1) openLink(links[0] as string);
    else if (links.length > 1) batch.show(links.join('\n'));
  }

  function label(route: Route) {
    return t(`nav.${route}`);
  }
</script>

<svelte:window {onkeydown} />

{#if failedToStart}
  <div class="boot">
    <Logo size={56} />
    <p>{failedToStart}</p>
  </div>
{:else if !app.ready}
  <div class="boot"><Logo size={56} /><Spinner size={20} /></div>
{:else}
  <div
    class="layout"
    role="application"
    {ondragover}
    ondragleave={(e) => {
      if (e.relatedTarget === null) dragging = false;
    }}
    {ondrop}
  >
    <nav aria-label={t('app.name')}>
      <div class="brand">
        <Logo size={34} />
        <div>
          <strong>{t('app.name')}</strong>
          <span class="faint mono-num">v{app.info?.version}</span>
        </div>
      </div>
      {#each GROUPS as group (group)}
        <ul>
          {#each ROUTES.filter((r) => r.group === group) as item, i (item.id)}
            <li>
              <a
                href="#{item.id}"
                class:active={router.route === item.id}
                aria-current={router.route === item.id ? 'page' : undefined}
                title="{label(item.id)} (Ctrl+{ROUTES.indexOf(item) + 1})"
                onclick={(e) => {
                  e.preventDefault();
                  go(item.id);
                }}
                data-index={i}
              >
                <Icon name={item.icon} />
                <span>{label(item.id)}</span>
                {#if item.id === 'downloads' && progress.active > 0}
                  <span class="badge mono-num">{progress.active}</span>
                {:else if item.id === 'watchlist' && app.watchNew > 0}
                  <span class="badge ok mono-num">{app.watchNew}</span>
                {:else if item.id === 'about' && app.update?.available}
                  <span class="badge ok"><Icon name="update" /></span>
                {/if}
              </a>
            </li>
          {/each}
        </ul>
      {/each}
      <div class="nav-foot">
        {#if progress.active > 0}
          <button
            class="mini-progress"
            style="--p:{progress.ratio}"
            onclick={() => go('downloads')}
          >
            <span class="row">
              <span>{t('nav.activeJobs', { count: progress.active })}</span>
              <span class="mono-num">{Math.round(progress.ratio * 100)}%</span>
            </span>
            <span class="bar"><span></span></span>
          </button>
        {/if}
        <SupportCard variant="compact" />
      </div>
    </nav>

    <main class:flush={router.route === 'browse'}>
      {#key router.route}
        <div
          class="page"
          class:wide={router.route === 'browse'}
          in:fly={{ y: 8, duration: 240, easing: cubicOut }}
        >
          {#if router.route === 'browse'}
            <BrowsePage />
          {:else if router.route === 'downloads'}
            <DownloadsPage />
          {:else if router.route === 'history'}
            <HistoryPage />
          {:else if router.route === 'watchlist'}
            <WatchlistPage />
          {:else if router.route === 'favorites'}
            <FavoritesPage />
          {:else if router.route === 'logs'}
            <LogsPage />
          {:else if router.route === 'settings'}
            <SettingsPage />
          {:else if router.route === 'help'}
            <HelpPage />
          {:else}
            <AboutPage />
          {/if}
        </div>
      {/key}
    </main>

    {#if dragging}
      <div class="drop" aria-hidden="true">
        <div><Icon name="link" size={28} /><span>{t('browse.dropHint')}</span></div>
      </div>
    {/if}
  </div>
  <BatchDialog />
{/if}
<Toaster />

<style>
  .boot {
    height: 100vh;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 18px;
    color: var(--text-dim);
  }
  .layout {
    display: grid;
    grid-template-columns: 232px minmax(0, 1fr);
    height: 100vh;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 18px 12px 14px;
    border-right: 1px solid var(--border);
    background: var(--bg-elev);
    overflow-y: auto;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 0 6px 4px;
  }
  .brand div {
    display: grid;
    line-height: 1.3;
  }
  .brand strong {
    font-size: var(--fs-md);
  }
  .brand span {
    font-size: var(--fs-xs);
  }
  ul {
    list-style: none;
    padding: 0 0 12px;
    margin: 0;
    display: grid;
    gap: 2px;
    border-bottom: 1px solid var(--border);
  }
  ul:last-of-type {
    border-bottom: 0;
  }
  a {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    border-radius: var(--radius-md);
    color: var(--text-dim);
    font-weight: 600;
    text-decoration: none;
    position: relative;
    transition:
      background-color var(--dur-fast) var(--ease),
      color var(--dur-fast) var(--ease);
  }
  a:hover {
    color: var(--text);
    background: var(--bg-hover);
    text-decoration: none;
  }
  a.active {
    color: var(--text);
    background: var(--accent-soft);
  }
  a.active::before {
    content: '';
    position: absolute;
    left: -12px;
    top: 8px;
    bottom: 8px;
    width: 3px;
    border-radius: 0 3px 3px 0;
    background: var(--accent);
  }
  a.active :global(.icon) {
    color: var(--accent-strong);
  }
  .badge {
    margin-left: auto;
    min-width: 22px;
    padding: 0 7px;
    border-radius: 99px;
    background: var(--accent);
    color: var(--accent-contrast);
    font-size: var(--fs-xs);
    line-height: 20px;
    text-align: center;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .badge.ok {
    background: var(--success);
    color: #062b1b;
  }
  .nav-foot {
    margin-top: auto;
    display: grid;
    gap: 12px;
    padding: 0 4px;
    font-size: var(--fs-xs);
    color: var(--text-faint);
  }
  .mini-progress {
    display: grid;
    gap: 6px;
    color: var(--text-dim);
    background: none;
    border: 0;
    padding: 0 2px;
    text-align: left;
    font-size: var(--fs-xs);
  }
  .mini-progress .row {
    display: flex;
    justify-content: space-between;
  }
  .bar {
    height: 4px;
    border-radius: 99px;
    background: var(--bg-elev-3);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    width: 100%;
    background: var(--accent);
    transform-origin: left;
    transform: scaleX(var(--p));
    transition: transform 500ms var(--ease-out);
  }
  main {
    min-width: 0;
    overflow-y: auto;
    scrollbar-gutter: stable;
    padding: 28px clamp(20px, 3.4vw, 48px) 56px;
  }
  main.flush {
    padding-bottom: 0;
  }
  .page {
    max-width: 1040px;
    margin: 0 auto;
  }
  .page.wide {
    max-width: 1240px;
  }
  .drop {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: var(--scrim);
    backdrop-filter: blur(3px);
    z-index: 50;
    animation: pdl-fade-in var(--dur) var(--ease);
    pointer-events: none;
  }
  .drop div {
    display: grid;
    justify-items: center;
    gap: 12px;
    padding: 28px 40px;
    border-radius: var(--radius-xl);
    border: 2px dashed var(--accent);
    background: var(--bg-elev);
    color: var(--text);
    font-weight: 600;
    box-shadow: var(--shadow-lg);
  }
</style>
