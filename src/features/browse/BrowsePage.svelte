<script lang="ts">
  import { onMount } from 'svelte';
  import { fade } from 'svelte/transition';
  import { api } from '../../lib/api';
  import type { PageRef } from '../../lib/types';
  import { app, fmt, overallProgress, t, updateSettings } from '../../ui/state/app.svelte';
  import { browse, go, openLink } from '../../ui/state/router.svelte';
  import { toast } from '../../ui/state/toasts.svelte';
  import Button from '../../ui/components/Button.svelte';
  import Icon from '../../ui/components/Icon.svelte';
  import IconButton from '../../ui/components/IconButton.svelte';
  import Logo from '../../ui/components/Logo.svelte';
  import CreatorSearch from './CreatorSearch.svelte';
  import CreatorView from './CreatorView.svelte';
  import ListingView from './ListingView.svelte';
  import PostView from './PostView.svelte';
  import { batch, clearRecent, loadRecent, type RecentLink } from './batch.svelte';

  let input = $state(browse.input);
  type Current = PageRef | { kind: 'search'; query: string };
  let current = $state<Current | null>(null);
  let invalid = $state(false);
  let field = $state<HTMLInputElement>();
  let recent = $state<RecentLink[]>(loadRecent());
  const progress = $derived(overallProgress(app.jobs));

  async function open(text = input) {
    const value = text.trim();
    if (!value) {
      current = null;
      return;
    }
    const links = await api.parseLinks(value).catch(() => []);
    if (links.length > 1) {
      batch.show(value);
      return;
    }
    const ref = links[0]?.ref;
    if (!ref || ref.kind === 'other') {
      // Plain words search creators by name; other web addresses are wrong.
      if (/^(https?:\/\/|www\.)|\.[a-z]{2,}\//i.test(value) || value.length > 120) {
        invalid = true;
        toast(t('browse.invalid'), 'warning');
        return;
      }
      invalid = false;
      current = { kind: 'search', query: value };
      return;
    }
    invalid = false;
    input = links[0]?.url ?? value;
    current = ref;
  }

  // Links opened from elsewhere (history, favorites, watchlist, drops).
  let handled = -1;
  $effect(() => {
    if (browse.nonce !== handled) {
      handled = browse.nonce;
      if (browse.input) {
        input = browse.input;
        void open(browse.input);
      }
    }
  });

  async function paste() {
    try {
      const text = await navigator.clipboard.readText();
      if (text) {
        input = text.trim();
        void open(input);
      }
    } catch {
      field?.focus();
    }
  }

  function home() {
    current = null;
    input = '';
    recent = loadRecent();
  }

  onMount(() => {
    const focus = () => field?.focus();
    // Ctrl+V anywhere on the page (outside text fields) opens the pasted link.
    const onPaste = (event: ClipboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.closest('input, textarea, [contenteditable]')) return;
      const text = event.clipboardData?.getData('text')?.trim() ?? '';
      if (!text.includes('pawchive.pw')) return;
      event.preventDefault();
      input = text;
      void open(text);
    };
    window.addEventListener('pdl:focus-link', focus);
    window.addEventListener('paste', onPaste);
    if (!current) field?.focus();
    return () => {
      window.removeEventListener('pdl:focus-link', focus);
      window.removeEventListener('paste', onPaste);
    };
  });

  const kindIcon = { post: 'document', creator: 'account', listing: 'list' } as const;
</script>

<div class="browse">
  <form
    class="linkbar"
    class:invalid
    onsubmit={(e) => {
      e.preventDefault();
      void open();
    }}
  >
    {#if current}
      <IconButton icon="chevronLeft" label={t('browse.home')} onclick={home} />
    {:else}
      <span class="lead"><Icon name="link" /></span>
    {/if}
    <input
      bind:this={field}
      bind:value={input}
      class="selectable"
      type="text"
      spellcheck="false"
      autocomplete="off"
      placeholder={t('browse.placeholder')}
      aria-label={t('browse.placeholder')}
      oninput={() => (invalid = false)}
      onpaste={(e) => {
        const text = e.clipboardData?.getData('text') ?? '';
        if (text.includes('pawchive.pw')) {
          e.preventDefault();
          input = text.trim();
          void open(input);
        }
      }}
    />
    {#if !input}
      <Button size="sm" variant="ghost" icon="clipboard" onclick={paste}>
        {t('browse.paste')}
      </Button>
    {/if}
    <Button size="sm" variant="primary" type="submit" icon="arrowRight" disabled={!input.trim()}>
      {t('browse.open')}
    </Button>
    <span class="divider"></span>
    <Button
      size="sm"
      variant="ghost"
      icon="playlistPlus"
      onclick={() => batch.show('')}
      title="Ctrl+Shift+B"
    >
      {t('browse.batch')}
    </Button>
  </form>

  {#if current?.kind === 'post'}
    {#key `${current.service}/${current.creatorId}/${current.postId}`}
      <PostView service={current.service} creatorId={current.creatorId} postId={current.postId} />
    {/key}
  {:else if current?.kind === 'creator'}
    {#key `${current.service}/${current.creatorId}/${current.query}`}
      <CreatorView
        service={current.service}
        creatorId={current.creatorId}
        initialQuery={current.query}
      />
    {/key}
  {:else if current?.kind === 'listing'}
    <ListingView initialQuery={current.query} />
  {:else if current?.kind === 'search'}
    <CreatorSearch query={current.query} />
  {:else}
    <section class="welcome" in:fade={{ duration: 200 }}>
      <div class="hello">
        <Logo size={72} />
        <div>
          <h1>{t('browse.welcomeTitle')}</h1>
          <p>{t('browse.welcomeBody')}</p>
        </div>
      </div>

      <div class="cards">
        <button class="card" onclick={() => openLink('https://pawchive.pw/posts')}>
          <Icon name="web" size={22} />
          <strong>{t('browse.recentPosts')}</strong>
          <span>{t('browse.recentPostsBody')}</span>
        </button>
        <button class="card" onclick={() => (current = { kind: 'search', query: '' })}>
          <Icon name="search" size={22} />
          <strong>{t('browse.findCreators')}</strong>
          <span>{t('browse.findCreatorsBody')}</span>
        </button>
        <button class="card" onclick={() => batch.show('')}>
          <Icon name="playlistPlus" size={22} />
          <strong>{t('browse.batchTitle')}</strong>
          <span>{t('browse.batchBody')}</span>
        </button>
        <button class="card" onclick={() => go('downloads')}>
          <Icon name="queue" size={22} />
          <strong>{t('nav.downloads')}</strong>
          <span>
            {#if progress.active}
              {t('nav.activeJobs', { count: progress.active })}, {Math.round(progress.ratio * 100)}%
            {:else}
              {t('browse.queueIdle')}
            {/if}
          </span>
        </button>
        <button class="card" onclick={() => go('watchlist')}>
          <Icon name={app.watchNew ? 'bellRing' : 'eye'} size={22} />
          <strong>{t('nav.watchlist')}</strong>
          <span>
            {app.watchNew ? t('watch.newCount', { count: app.watchNew }) : t('browse.watchBody')}
          </span>
        </button>
      </div>

      <div class="fast">
        <div>
          <strong><Icon name="lightning" /> {t('settings.fastMode')}</strong>
          <span>{t('settings.fastModeHint')}</span>
        </div>
        <Button
          size="sm"
          variant={app.settings.fastMode ? 'primary' : 'secondary'}
          onclick={() => updateSettings({ fastMode: !app.settings.fastMode })}
        >
          {app.settings.fastMode ? t('browse.fastOn') : t('browse.fastOff')}
        </Button>
      </div>

      {#if recent.length}
        <div class="recent">
          <div class="recent-head">
            <h3>{t('browse.recent')}</h3>
            <button
              class="clear"
              onclick={() => {
                clearRecent();
                recent = [];
              }}
            >
              {t('common.clear')}
            </button>
          </div>
          <ul>
            {#each recent as link (link.url)}
              <li>
                <button onclick={() => openLink(link.url)}>
                  <Icon name={kindIcon[link.kind]} />
                  <span class="truncate">{link.title}</span>
                  <span class="faint when">{fmt.relative(link.at)}</span>
                </button>
              </li>
            {/each}
          </ul>
        </div>
      {/if}

      <p class="tips faint">
        <Icon name="keyboard" />
        {t('browse.tips')}
      </p>
    </section>
  {/if}
</div>

<style>
  .browse {
    display: grid;
    gap: 22px;
  }
  .linkbar {
    position: sticky;
    top: -28px;
    z-index: 6;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 8px 8px 12px;
    margin: -8px -4px 0;
    border-radius: var(--radius-lg);
    border: 1px solid var(--border-strong);
    background: color-mix(in srgb, var(--bg-elev) 92%, transparent);
    backdrop-filter: blur(10px);
    box-shadow: var(--shadow-sm);
    transition:
      border-color var(--dur-fast) var(--ease),
      box-shadow var(--dur-fast) var(--ease);
  }
  .linkbar:focus-within {
    border-color: var(--accent);
    box-shadow: var(--focus-ring);
  }
  .linkbar.invalid {
    border-color: var(--danger);
  }
  .lead {
    display: grid;
    place-items: center;
    width: 30px;
    color: var(--text-faint);
  }
  input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: var(--fs-md);
    padding: 6px 2px;
  }
  input:focus-visible {
    box-shadow: none;
  }
  input::placeholder {
    color: var(--text-faint);
  }
  .divider {
    width: 1px;
    align-self: stretch;
    margin: 4px 2px;
    background: var(--border);
  }
  .welcome {
    display: grid;
    gap: 22px;
    max-width: 900px;
  }
  .hello {
    display: flex;
    align-items: center;
    gap: 20px;
    padding: 8px 4px 0;
  }
  .hello :global(.logo) {
    transform: rotate(-6deg);
  }
  h1 {
    font-size: var(--fs-2xl);
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .hello p {
    color: var(--text-dim);
    max-width: 620px;
    margin-top: 4px;
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));
    gap: 12px;
  }
  .card {
    display: grid;
    gap: 6px;
    justify-items: start;
    text-align: left;
    padding: 16px;
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    background: var(--bg-elev);
    color: var(--text);
    transition:
      border-color var(--dur-fast) var(--ease),
      transform var(--dur) var(--ease-spring),
      background-color var(--dur-fast) var(--ease);
  }
  .card:hover {
    border-color: color-mix(in srgb, var(--accent) 55%, var(--border));
    transform: translateY(-2px);
    background: var(--bg-elev-2);
  }
  .card :global(.icon) {
    color: var(--accent-strong);
    margin-bottom: 4px;
  }
  .card span {
    font-size: var(--fs-sm);
    color: var(--text-dim);
  }
  .fast {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 14px 16px;
    border-radius: var(--radius-lg);
    border: 1px dashed color-mix(in srgb, var(--accent) 45%, var(--border-strong));
    background: var(--accent-softer);
  }
  .fast div {
    display: grid;
    gap: 2px;
  }
  .fast strong {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .fast span {
    font-size: var(--fs-sm);
    color: var(--text-dim);
  }
  .recent {
    display: grid;
    gap: 8px;
  }
  .recent-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  h3 {
    font-size: var(--fs-md);
    font-weight: 700;
  }
  .clear {
    border: 0;
    background: none;
    color: var(--text-faint);
    font-size: var(--fs-xs);
  }
  .clear:hover {
    color: var(--text);
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 6px;
  }
  li button {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 12px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: var(--bg-elev);
    color: var(--text);
    text-align: left;
  }
  li button:hover {
    border-color: var(--border-strong);
    background: var(--bg-elev-2);
  }
  li :global(.icon) {
    color: var(--text-faint);
  }
  .when {
    margin-left: auto;
    flex: none;
    font-size: var(--fs-xs);
  }
  .tips {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: var(--fs-xs);
  }
</style>
