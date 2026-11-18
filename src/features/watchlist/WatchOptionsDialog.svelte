<script lang="ts">
  import { api, pickFolder } from '../../lib/api';
  import {
    EMPTY_FILTER,
    type DownloadMode,
    type PostFilter,
    type WatchEntry,
  } from '../../lib/types';
  import { app, t } from '../../ui/state/app.svelte';
  import { toast } from '../../ui/state/toasts.svelte';
  import { describeError } from '../../ui/state/errors';
  import Button from '../../ui/components/Button.svelte';
  import Dialog from '../../ui/components/Dialog.svelte';
  import Icon from '../../ui/components/Icon.svelte';
  import Segmented from '../../ui/components/Segmented.svelte';
  import Switch from '../../ui/components/Switch.svelte';
  import FilterEditor from '../../ui/layout/FilterEditor.svelte';

  interface Props {
    entry: WatchEntry | null;
    onclose: (saved: boolean) => void;
  }

  let { entry, onclose }: Props = $props();

  let folder = $state<string | null>(null);
  let mode = $state<DownloadMode | 'default'>('default');
  let useFilter = $state(false);
  let filter = $state<PostFilter>({ ...EMPTY_FILTER });
  let saving = $state(false);
  let loadedFor: string | null = null;

  // Load the entry's options each time the dialog opens for one.
  $effect(() => {
    if (!entry || loadedFor === entry.key) return;
    loadedFor = entry.key;
    const o = entry.options ?? {};
    folder = o.downloadDir ?? null;
    mode = o.mode ?? 'default';
    useFilter = Boolean(o.filter);
    filter = o.filter ? { ...o.filter } : structuredClone($state.snapshot(app.settings.filter));
  });

  async function chooseFolder() {
    const picked = await pickFolder(t('settings.downloadDir'), folder ?? app.settings.downloadDir);
    if (picked) folder = picked;
  }

  async function save() {
    if (!entry) return;
    saving = true;
    try {
      await api.setWatchOptions(entry.key, {
        downloadDir: folder,
        mode: mode === 'default' ? null : mode,
        filter: useFilter ? $state.snapshot(filter) : null,
      });
      toast(t('watch.optionsSaved'), 'success', { duration: 1400 });
      loadedFor = null;
      onclose(true);
    } catch (e) {
      toast(describeError(e), 'error');
    } finally {
      saving = false;
    }
  }
</script>

<Dialog
  open={entry !== null}
  title={t('watch.optionsTitle', { name: entry?.name ?? '' })}
  width={580}
  confirmLabel={t('common.save')}
  busy={saving}
  onconfirm={save}
  oncancel={() => {
    loadedFor = null;
    onclose(false);
  }}
>
  <p class="note">{t('watch.optionsHint')}</p>
  <div class="row">
    <span class="label">{t('settings.downloadDir')}</span>
    <div class="folder">
      <button type="button" class="pick" onclick={chooseFolder} title={folder ?? ''}>
        <Icon name="folder" />
        <span class="truncate">{folder ?? t('watch.sameFolder')}</span>
      </button>
      {#if folder}
        <Button size="sm" variant="ghost" onclick={() => (folder = null)}>
          {t('watch.useDefault')}
        </Button>
      {/if}
    </div>
  </div>
  <div class="row">
    <span class="label">{t('options.mode')}</span>
    <Segmented
      size="sm"
      label={t('options.mode')}
      bind:value={mode}
      options={[
        { value: 'default', label: t('watch.modeDefault') },
        { value: 'individual', label: t('options.individual') },
        { value: 'zip', label: t('options.zip') },
      ]}
    />
  </div>
  <label class="row switch">
    <span class="label">{t('watch.ownFilter')}</span>
    <Switch label={t('watch.ownFilter')} checked={useFilter} onchange={(v) => (useFilter = v)} />
  </label>
  {#if useFilter}
    <FilterEditor value={filter} onchange={(v) => (filter = v)} />
  {/if}
</Dialog>

<style>
  .note {
    margin: 0 0 14px;
    font-size: var(--fs-sm);
    color: var(--text-dim);
  }
  .row {
    display: grid;
    gap: 6px;
    margin-bottom: 14px;
  }
  .row.switch {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .label {
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--text-dim);
  }
  .folder {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .pick {
    flex: 1;
    min-width: 0;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 10px;
    border-radius: var(--radius-md);
    border: 1px dashed var(--border-strong);
    background: transparent;
    color: var(--text);
    font-size: var(--fs-sm);
  }
  .pick:hover {
    border-color: var(--accent);
  }
</style>
