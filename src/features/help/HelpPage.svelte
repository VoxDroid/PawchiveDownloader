<script lang="ts">
  import { slide } from 'svelte/transition';
  import { openUrl } from '../../lib/api';
  import { LINKS } from '../../lib/links';
  import type { IconName } from '../../ui/icons.generated';
  import { t } from '../../ui/state/app.svelte';
  import { go } from '../../ui/state/router.svelte';
  import Button from '../../ui/components/Button.svelte';
  import Empty from '../../ui/components/Empty.svelte';
  import Icon from '../../ui/components/Icon.svelte';
  import TextInput from '../../ui/components/TextInput.svelte';
  import PageHeader from '../../ui/layout/PageHeader.svelte';

  const TOPICS: Array<{ id: string; icon: IconName; group: 'basics' | 'features' | 'help' }> = [
    { id: 'start', icon: 'rocket', group: 'basics' },
    { id: 'post', icon: 'document', group: 'basics' },
    { id: 'creator', icon: 'account', group: 'basics' },
    { id: 'batch', icon: 'playlistPlus', group: 'basics' },
    { id: 'search', icon: 'search', group: 'basics' },
    { id: 'sites', icon: 'earth', group: 'basics' },
    { id: 'formats', icon: 'folderZip', group: 'features' },
    { id: 'queue', icon: 'queue', group: 'features' },
    { id: 'resume', icon: 'fileCheck', group: 'features' },
    { id: 'preview', icon: 'eyeOff', group: 'features' },
    { id: 'watch', icon: 'eye', group: 'features' },
    { id: 'favorites', icon: 'starOutline', group: 'features' },
    { id: 'filters', icon: 'filter', group: 'features' },
    { id: 'naming', icon: 'rename', group: 'features' },
    { id: 'schedule', icon: 'clock', group: 'features' },
    { id: 'library', icon: 'database', group: 'features' },
    { id: 'outside', icon: 'external', group: 'features' },
    { id: 'gallery', icon: 'thumbnails', group: 'features' },
    { id: 'portable', icon: 'harddisk', group: 'features' },
    { id: 'bridge', icon: 'web', group: 'features' },
    { id: 'network', icon: 'network', group: 'features' },
    { id: 'tray', icon: 'tray', group: 'features' },
    { id: 'polite', icon: 'shield', group: 'help' },
    { id: 'trouble', icon: 'warning', group: 'help' },
    { id: 'privacy', icon: 'shieldLock', group: 'help' },
  ];

  let query = $state('');
  let open = $state<Set<string>>(new Set(['start']));

  const shown = $derived(
    TOPICS.filter((topic) => {
      const q = query.trim().toLowerCase();
      if (!q) return true;
      return `${t(`help.${topic.id}.title`)} ${t(`help.${topic.id}.body`)}`
        .toLowerCase()
        .includes(q);
    }),
  );

  function toggle(id: string) {
    const next = new Set(open);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    open = next;
  }

  function paragraphs(id: string) {
    return t(`help.${id}.body`).split('\n').filter(Boolean);
  }
</script>

<PageHeader title={t('nav.help')} description={t('help.description')}>
  {#snippet actions()}
    <Button icon="bug" onclick={() => openUrl(LINKS.issues)}>{t('about.report')}</Button>
    <Button icon="logs" onclick={() => go('logs')}>{t('nav.logs')}</Button>
  {/snippet}
</PageHeader>

<div class="search">
  <TextInput
    label={t('help.search')}
    placeholder={t('help.search')}
    icon="search"
    bind:value={query}
    clearable
  />
</div>

{#if shown.length === 0}
  <Empty compact icon="search" title={t('history.noMatches')} />
{/if}

{#each ['basics', 'features', 'help'] as group (group)}
  {@const topics = shown.filter((topic) => topic.group === group)}
  {#if topics.length}
    <h2 class="group">{t(`help.group.${group}`)}</h2>
    <div class="topics">
      {#each topics as topic (topic.id)}
        {@const expanded = open.has(topic.id) || query.trim().length > 0}
        <section class="topic" class:expanded>
          <button class="head" aria-expanded={expanded} onclick={() => toggle(topic.id)}>
            <span class="icon-box"><Icon name={topic.icon} /></span>
            <span class="title">{t(`help.${topic.id}.title`)}</span>
            <Icon name={expanded ? 'chevronUp' : 'chevronDown'} class="chev" />
          </button>
          {#if expanded}
            <div class="body selectable" transition:slide={{ duration: 200 }}>
              {#each paragraphs(topic.id) as text, i (i)}
                <p>{text}</p>
              {/each}
            </div>
          {/if}
        </section>
      {/each}
    </div>
  {/if}
{/each}

<style>
  .search {
    max-width: 460px;
    margin-bottom: 20px;
  }
  .group {
    font-size: var(--fs-sm);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-faint);
    margin: 22px 4px 10px;
  }
  .topics {
    display: grid;
    gap: 8px;
  }
  .topic {
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    background: var(--bg-elev);
    overflow: hidden;
    transition: border-color var(--dur-fast) var(--ease);
  }
  .topic.expanded {
    border-color: var(--border-strong);
  }
  .head {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    border: 0;
    background: none;
    color: var(--text);
    text-align: left;
  }
  .head:hover {
    background: var(--bg-hover);
  }
  .icon-box {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 9px;
    background: var(--accent-softer);
    color: var(--accent-strong);
    flex: none;
  }
  .title {
    flex: 1;
    font-weight: 700;
  }
  .head :global(.chev) {
    color: var(--text-faint);
  }
  .body {
    display: grid;
    gap: 10px;
    padding: 0 18px 16px 56px;
    color: var(--text-dim);
    line-height: 1.7;
    max-width: 820px;
  }
</style>
