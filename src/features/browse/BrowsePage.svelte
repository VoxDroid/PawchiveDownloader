<script lang="ts">
  import { onMount, tick } from 'svelte';
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
  import { mentionsSite } from '../../lib/urls';

  let { active = true }: { active?: boolean } = $props();

  let input = $state(browse.input);
  type Current = PageRef | { kind: 'search'; query: string };
  /** One page opened in Browse. Earlier ones stay loaded underneath. */
  interface View {
    id: number;
    current: Current;
    input: string;
    title: string;
    scroll: number;
  }
  /** How many opened pages stay loaded for going back. */
  const MAX_VIEWS = 8;
  let views = $state<View[]>([]);
  let nextId = 0;
  const top = $derived(views.at(-1) ?? null);
  const current = $derived(top?.current ?? null);
  /** Pages left with Back, for going forward again. */
  let ahead = $state<Array<{ current: Current; input: string; title: string }>>([]);
  let invalid = $state(false);
  let field = $state<HTMLInputElement>();
  let recent = $state<RecentLink[]>(loadRecent());
  const progress = $derived(overallProgress(app.jobs));

  const scroller = () => field?.closest('main') ?? null;

  function keyOf(c: Current): string {
    switch (c.kind) {
      case 'post':
        return `post/${c.service}/${c.creatorId}/${c.postId}`;
      case 'creator':
        return `creator/${c.service}/${c.creatorId}/${c.query ?? ''}`;
      case 'listing':
        return `listing/${c.site ?? 'pawchive'}/${c.query ?? ''}`;
      case 'search':
        return `search/${c.query}`;
      default:
        return 'other';
    }
  }

  function fallbackTitle(c: Current): string {
    switch (c.kind) {
      case 'post':
        return t('browse.crumbPost');
      case 'creator':
        return t('browse.crumbCreator');
      case 'listing':
        return t('browse.recentPosts');
      case 'search':
        return c.query ? `"${c.query}"` : t('browse.findCreators');
      default:
        return '';
    }
  }

  function scrollTo(y: number) {
    void tick().then(() => {
      const main = scroller();
      if (main) main.scrollTop = y;
    });
  }

  function keepScroll() {
    const main = scroller();
    if (top && main) top.scroll = main.scrollTop;
  }

  /** Shows a page: one already loaded comes back as it was, others load. */
  function show(next: Current, text: string, title = '', fresh = true) {
    const key = keyOf(next);
    const at = views.findIndex((v) => keyOf(v.current) === key);
    if (top && at === views.length - 1) {
      top.input = text;
      return;
    }
    keepScroll();
    if (fresh) ahead = [];
    if (at >= 0) {
      // Opened before and still loaded: go back to it as it was.
      const kept = views[at] as View;
      views = views.slice(0, at + 1);
      kept.input = text;
      scrollTo(kept.scroll);
      return;
    }
    const view: View = { id: nextId++, current: next, input: text, title, scroll: 0 };
    views = [...views, view].slice(-MAX_VIEWS);
    scrollTo(0);
  }

  function setTitle(id: number, title: string) {
    const view = views.find((v) => v.id === id);
    if (view) view.title = title;
  }

  /** Back to the page before, still loaded; from the first page, the start. */
  function back() {
    const leaving = views.at(-1);
    if (!leaving) return;
    ahead = [
      { current: leaving.current, input: leaving.input, title: leaving.title },
      ...ahead,
    ].slice(0, MAX_VIEWS);
    views = views.slice(0, -1);
    const now = views.at(-1);
    input = now?.input ?? '';
    invalid = false;
    if (now) scrollTo(now.scroll);
    else {
      recent = loadRecent();
      scrollTo(0);
    }
  }

  function forward() {
    const [next, ...rest] = ahead;
    if (!next) return;
    ahead = rest;
    input = next.input;
    show(next.current, next.input, next.title, false);
  }

  /** Goes back to the page at `index` in the trail (-1 for the start). */
  function backTo(index: number) {
    while (views.length - 1 > index) back();
  }

  // Only the newest lookup counts, so a slow one cannot replace what was
  // typed after it.
  let openRun = 0;

  async function open(text = input) {
    const run = ++openRun;
    const value = text.trim();
    if (!value) {
      home();
      return;
    }
    const links = await api.parseLinks(value).catch(() => []);
    if (run !== openRun) return;
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
      show({ kind: 'search', query: value }, value);
      return;
    }
    invalid = false;
    input = links[0]?.url ?? value;
    show(ref, input);
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
    if (views.length) {
      keepScroll();
      ahead = [
        ...[...views]
          .reverse()
          .map((v) => ({ current: v.current, input: v.input, title: v.title })),
        ...ahead,
      ].slice(0, MAX_VIEWS);
    }
    views = [];
    input = '';
    invalid = false;
    recent = loadRecent();
    scrollTo(0);
  }

  /** A dialog or the image viewer is open; keys and buttons belong to it. */
  const modalOpen = () =>
    [...document.querySelectorAll('[aria-modal="true"]')].some((el) => el.checkVisibility());

  onMount(() => {
    const focus = () => field?.focus();
    // Ctrl+V anywhere on the page (outside text fields) opens the pasted link.
    const onPaste = (event: ClipboardEvent) => {
      if (!active) return;
      const target = event.target as HTMLElement | null;
      if (target?.closest('input, textarea, [contenteditable]')) return;
      const text = event.clipboardData?.getData('text')?.trim() ?? '';
      if (!mentionsSite(text)) return;
      event.preventDefault();
      input = text;
      void open(text);
    };
    // Alt+Left and Alt+Right, and the mouse's back and forward buttons, move
    // through the pages opened here, like a web browser.
    const onKey = (event: KeyboardEvent) => {
      if (!active || !event.altKey || event.ctrlKey || event.metaKey || modalOpen()) return;
      if (event.key === 'ArrowLeft' && views.length) {
        event.preventDefault();
        back();
      } else if (event.key === 'ArrowRight' && ahead.length) {
        event.preventDefault();
        forward();
      }
    };
    const onMouse = (event: MouseEvent) => {
      if (!active || modalOpen()) return;
      if (event.button === 3 && views.length) {
        event.preventDefault();
        if (event.type === 'mouseup') back();
      } else if (event.button === 4 && ahead.length) {
        event.preventDefault();
        if (event.type === 'mouseup') forward();
      }
    };
    window.addEventListener('pdl:focus-link', focus);
    window.addEventListener('paste', onPaste);
    window.addEventListener('keydown', onKey);
    window.addEventListener('mousedown', onMouse);
    window.addEventListener('mouseup', onMouse);
    if (!current) field?.focus();
    return () => {
      window.removeEventListener('pdl:focus-link', focus);
      window.removeEventListener('paste', onPaste);
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('mousedown', onMouse);
      window.removeEventListener('mouseup', onMouse);
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
      <IconButton icon="chevronLeft" label={t('browse.back')} onclick={back} />
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
        if (mentionsSite(text)) {
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

  {#if views.length}
    <nav class="trail" aria-label={t('browse.trail')}>
      <button class="crumb home" onclick={home} data-tip={t('browse.home')}>
        <Icon name="compass" />
        <span>{t('browse.start')}</span>
      </button>
      {#each views as view, i (view.id)}
        <Icon name="chevronRight" size={14} />
        {#if i === views.length - 1}
          <span class="crumb now truncate" aria-current="page">
            {view.title || fallbackTitle(view.current)}
          </span>
        {:else}
          <button class="crumb truncate" onclick={() => backTo(i)}>
            {view.title || fallbackTitle(view.current)}
          </button>
        {/if}
      {/each}
      {#if ahead.length}
        <span class="spacer"></span>
        <IconButton
          icon="chevronRight"
          size="sm"
          label={t('browse.forward', {
            title: ahead[0]?.title || fallbackTitle(ahead[0]!.current),
          })}
          tipSide="left"
          onclick={forward}
        />
      {/if}
    </nav>
  {/if}

  {#each views as view, i (view.id)}
    {@const c = view.current}
    <div class="view" hidden={i !== views.length - 1}>
      {#if c.kind === 'post'}
        <PostView
          service={c.service}
          creatorId={c.creatorId}
          postId={c.postId}
          ontitle={(title) => setTitle(view.id, title)}
        />
      {:else if c.kind === 'creator'}
        <CreatorView
          service={c.service}
          creatorId={c.creatorId}
          initialQuery={c.query}
          ontitle={(title) => setTitle(view.id, title)}
        />
      {:else if c.kind === 'listing'}
        <ListingView initialQuery={c.query} initialSite={c.site ?? 'pawchive'} />
      {:else if c.kind === 'search'}
        <CreatorSearch query={c.query} />
      {/if}
    </div>
  {/each}

  {#if !views.length}
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
        <button class="card" onclick={() => show({ kind: 'search', query: '' }, '')}>
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
    /* The page has 28px of top padding and the bar a -8px margin, so it
       rests 20px below the window's edge; -8px keeps it right there while
       scrolling instead of sliding up against the title bar. */
    top: -8px;
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
  /* Covers the gap above the bar, so nothing scrolls past between the bar
     and the window's edge. */
  .linkbar::before {
    content: '';
    position: absolute;
    left: -16px;
    right: -16px;
    bottom: calc(100% + 1px);
    height: 24px;
    background: var(--bg);
    pointer-events: none;
    /* Behind the bar's own layer, so the focus ring draws over it. */
    z-index: -1;
  }
  /* The same ring as elsewhere, drawn as an outline. */
  .linkbar:focus-within {
    border-color: var(--accent);
    outline: 2px solid color-mix(in srgb, var(--accent) 70%, transparent);
    outline-offset: 2px;
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
  /* Where you came from: each earlier page is still loaded, so going back
     is instant and keeps your place. */
  .trail {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    margin: -10px 2px -8px;
    color: var(--text-faint);
    font-size: var(--fs-sm);
  }
  .trail > :global(.icon) {
    flex: none;
  }
  .crumb {
    min-width: 0;
    max-width: 260px;
    flex: 0 1 auto;
    padding: 3px 8px;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text-dim);
    font: inherit;
    transition:
      background-color var(--dur-fast) var(--ease),
      color var(--dur-fast) var(--ease);
  }
  button.crumb:hover {
    background: var(--bg-elev-2);
    color: var(--text);
  }
  .crumb.home {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: none;
  }
  .crumb.now {
    color: var(--text);
    font-weight: 600;
  }
  .trail .spacer {
    flex: 1;
  }
  .view[hidden] {
    display: none;
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
