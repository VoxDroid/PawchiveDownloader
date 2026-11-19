<script lang="ts">
  import type { Post } from '../../lib/types';
  import { t } from '../../ui/state/app.svelte';
  import { openLink } from '../../ui/state/router.svelte';
  import Empty from '../../ui/components/Empty.svelte';
  import VirtualList from '../../ui/components/VirtualList.svelte';
  import PostRow from './PostRow.svelte';

  interface Props {
    posts: Post[];
    selected: Set<string>;
    downloaded: Set<string>;
    /** Posts opened before. */
    seen?: Set<string>;
    showCreator?: boolean;
    onchange: (next: Set<string>) => void;
  }

  let {
    posts,
    selected,
    downloaded,
    seen = new Set(),
    showCreator = false,
    onchange,
  }: Props = $props();
  let last: string | null = null;

  function select(id: string, event: MouseEvent | KeyboardEvent) {
    const next = new Set(selected);
    if (event.shiftKey && last !== null) {
      const a = posts.findIndex((p) => p.id === last);
      const b = posts.findIndex((p) => p.id === id);
      const [from, to] = a < b ? [a, b] : [b, a];
      const on = !selected.has(id);
      for (const p of posts.slice(from, to + 1)) {
        if (on) next.add(p.id);
        else next.delete(p.id);
      }
    } else if (next.has(id)) next.delete(id);
    else next.add(id);
    last = id;
    onchange(next);
  }
</script>

<div class="list" role="listbox" aria-multiselectable="true">
  <VirtualList items={posts} rowHeight={58} key={(p) => p.id}>
    {#snippet row(post)}
      <PostRow
        {post}
        {showCreator}
        selected={selected.has(post.id)}
        seen={seen.has(post.id)}
        downloaded={downloaded.has(post.id)}
        onselect={(e) => select(post.id, e)}
        onopen={() => openLink(post.url)}
      />
    {/snippet}
    {#snippet empty()}
      <Empty compact icon="filter" title={t('creator.noMatches')} />
    {/snippet}
  </VirtualList>
</div>

<style>
  .list {
    height: min(62vh, calc(100vh - 360px));
    min-height: 280px;
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    background: var(--bg-elev);
    padding: 4px;
  }
</style>
