<script lang="ts">
  import type { Snippet } from 'svelte';
  import { pickFolder } from '../../lib/api';
  import type { DownloadMode, JobOverrides } from '../../lib/types';
  import { app, t } from '../../ui/state/app.svelte';
  import Button from '../../ui/components/Button.svelte';
  import Checkbox from '../../ui/components/Checkbox.svelte';
  import Icon from '../../ui/components/Icon.svelte';
  import Segmented from '../../ui/components/Segmented.svelte';

  interface Props {
    label: string;
    disabled?: boolean;
    busy?: boolean;
    summary?: string;
    ondownload: (options: JobOverrides) => void;
    extra?: Snippet;
  }

  let { label, disabled = false, busy = false, summary, ondownload, extra }: Props = $props();

  let mode = $state<DownloadMode>(app.settings.downloadMode);
  let saveText = $state(app.settings.saveContent !== 'none');
  let skipDownloaded = $state(app.settings.skipDownloaded);
  let folder = $state<string | null>(null);

  const destination = $derived(folder ?? app.settings.downloadDir);
  const shortDestination = $derived(
    destination.length > 42 ? `...${destination.slice(-40)}` : destination,
  );

  async function chooseFolder() {
    const picked = await pickFolder(t('settings.downloadDir'), destination);
    if (picked) folder = picked;
  }

  function submit() {
    ondownload({
      mode,
      saveContent: saveText
        ? app.settings.saveContent === 'none'
          ? 'txt'
          : app.settings.saveContent
        : 'none',
      skipDownloaded,
      ...(folder ? { downloadDir: folder } : {}),
    });
  }
</script>

<footer class="bar">
  <div class="options">
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
    <Checkbox bind:checked={saveText}>{t('options.saveText')}</Checkbox>
    <Checkbox bind:checked={skipDownloaded}>{t('options.skipDownloaded')}</Checkbox>
    <button
      type="button"
      class="folder"
      onclick={chooseFolder}
      data-tip={t('browse.changeFolder')}
      title={destination}
    >
      <Icon name="folder" /><span class="truncate">{shortDestination}</span>
    </button>
  </div>
  <div class="go">
    {#if summary}<span class="summary mono-num">{summary}</span>{/if}
    {#if extra}{@render extra()}{/if}
    <Button variant="primary" icon="download" loading={busy} {disabled} onclick={submit}>
      {label}
    </Button>
  </div>
</footer>

<style>
  .bar {
    position: sticky;
    bottom: 0;
    margin: 18px -16px 0;
    padding: 14px 16px 18px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 12px 20px;
    background: color-mix(in srgb, var(--bg) 82%, transparent);
    backdrop-filter: blur(10px);
    border-top: 1px solid var(--border);
    z-index: 5;
  }
  .options {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px 18px;
    min-width: 0;
  }
  .go {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-left: auto;
  }
  .summary {
    font-size: var(--fs-sm);
    color: var(--text-dim);
  }
  .folder {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    max-width: 280px;
    padding: 5px 10px;
    border-radius: 99px;
    border: 1px dashed var(--border-strong);
    background: transparent;
    color: var(--text-dim);
    font-size: var(--fs-xs);
    transition:
      color var(--dur-fast) var(--ease),
      border-color var(--dur-fast) var(--ease);
  }
  .folder:hover {
    color: var(--text);
    border-color: var(--accent);
  }
</style>
