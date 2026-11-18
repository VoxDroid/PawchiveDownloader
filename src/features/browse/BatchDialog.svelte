<script lang="ts">
  import { api, errorText, pickFile } from '../../lib/api';
  import type { DownloadMode, ParsedLink } from '../../lib/types';
  import { app, t } from '../../ui/state/app.svelte';
  import { go } from '../../ui/state/router.svelte';
  import { toast } from '../../ui/state/toasts.svelte';
  import Button from '../../ui/components/Button.svelte';
  import Dialog from '../../ui/components/Dialog.svelte';
  import Icon from '../../ui/components/Icon.svelte';
  import Segmented from '../../ui/components/Segmented.svelte';
  import { batch } from './batch.svelte';

  const PLACEHOLDER = [
    'https://pawchive.pw/patreon/user/...',
    'https://pawchive.pw/fanbox/user/.../post/...',
  ].join(String.fromCharCode(10));
  let parsed = $state<ParsedLink[]>([]);
  let mode = $state<DownloadMode>(app.settings.downloadMode);
  let busy = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    const text = batch.text;
    clearTimeout(timer);
    timer = setTimeout(() => {
      if (!text.trim()) {
        parsed = [];
        return;
      }
      void api
        .parseLinks(text)
        .then((links) => (parsed = links.filter((l) => l.ref.kind !== 'other')))
        .catch(() => (parsed = []));
    }, 180);
  });

  $effect(() => {
    if (batch.open) mode = app.settings.downloadMode;
  });

  const posts = $derived(parsed.filter((l) => l.ref.kind === 'post').length);
  const creators = $derived(parsed.filter((l) => l.ref.kind === 'creator').length);

  async function importFile() {
    const path = await pickFile(t('batch.importTitle'), [
      { name: t('batch.textFiles'), extensions: ['txt', 'csv', 'md', 'html'] },
    ]);
    if (!path) return;
    try {
      const text = await api.readTextFile(path);
      batch.text = batch.text.trim() ? `${batch.text.trim()}\n${text}` : text;
    } catch (e) {
      toast(errorText(e), 'error');
    }
  }

  async function queue() {
    busy = true;
    try {
      const results = await api.queueLinks(batch.text, { mode });
      const jobs = new Set(results.filter((r) => r.jobId).map((r) => r.jobId)).size;
      toast(t('batch.queued', { count: jobs }), 'success', {
        action: { label: t('nav.downloads'), run: () => go('downloads') },
      });
      batch.close();
      batch.text = '';
    } catch (e) {
      toast(errorText(e), 'error');
    } finally {
      busy = false;
    }
  }
</script>

<Dialog
  open={batch.open}
  width={620}
  title={t('batch.title')}
  confirmLabel={t('batch.queue', { count: parsed.length })}
  {busy}
  onconfirm={parsed.length ? queue : undefined}
  oncancel={() => batch.close()}
>
  <p>{t('batch.body')}</p>
  <textarea
    class="links selectable"
    rows="8"
    spellcheck="false"
    placeholder={PLACEHOLDER}
    bind:value={batch.text}></textarea>
  <div class="row">
    <span class="summary mono-num">
      {#if parsed.length}
        <Icon name="account" />{t('batch.creators', { count: creators })}
        <Icon name="document" />{t('batch.posts', { count: posts })}
      {:else}
        {t('batch.none')}
      {/if}
    </span>
    <Button size="sm" variant="ghost" icon="fileImport" onclick={importFile}>
      {t('batch.importFile')}
    </Button>
  </div>
  <Segmented
    label={t('options.mode')}
    size="sm"
    bind:value={mode}
    options={[
      { value: 'individual', label: t('options.individual'), icon: 'layers' },
      { value: 'zip', label: t('options.zip'), icon: 'folderZip' },
      { value: 'thumbnails', label: t('options.thumbnails'), icon: 'thumbnails' },
    ]}
  />
</Dialog>

<style>
  .links {
    width: 100%;
    resize: vertical;
    min-height: 140px;
    padding: 10px 12px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border-strong);
    background: var(--bg);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-sm);
    line-height: 1.6;
    outline: none;
    transition: border-color var(--dur-fast) var(--ease);
  }
  .links:focus {
    border-color: var(--accent);
    box-shadow: var(--focus-ring);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .summary {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-sm);
    color: var(--text-dim);
  }
  .summary :global(.icon:not(:first-child)) {
    margin-left: 10px;
  }
</style>
