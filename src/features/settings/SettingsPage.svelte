<script lang="ts">
  import { describeError } from '../../ui/state/errors';
  import { onMount } from 'svelte';
  import { api, inTauri, pickFolder, pickFile, pickSavePath } from '../../lib/api';
  import { openTextFile, saveTextFile, stamp } from '../../lib/files';
  import { LANGUAGE_NAMES } from '../../lib/i18n';
  import {
    buildArchivePath,
    buildFilePath,
    withoutWords,
    FILE_TEMPLATE_PRESETS,
    FOLDER_TEMPLATE_PRESETS,
    type NameContext,
  } from '../../lib/naming/template';
  import {
    ACCENTS,
    DEFAULT_SETTINGS,
    FILE_CATEGORIES,
    LOCALES,
    THEMES,
    WATCH_INTERVALS,
    type Counts,
    type ProxyMode,
    type Settings,
  } from '../../lib/types';
  import { app, fmt, replaceSettings, t, updateSettings } from '../../ui/state/app.svelte';
  import { router } from '../../ui/state/router.svelte';
  import { toast } from '../../ui/state/toasts.svelte';
  import Button from '../../ui/components/Button.svelte';
  import Chip from '../../ui/components/Chip.svelte';
  import Dialog from '../../ui/components/Dialog.svelte';
  import Icon from '../../ui/components/Icon.svelte';
  import Segmented from '../../ui/components/Segmented.svelte';
  import Select from '../../ui/components/Select.svelte';
  import Switch from '../../ui/components/Switch.svelte';
  import TextInput from '../../ui/components/TextInput.svelte';
  import PageHeader from '../../ui/layout/PageHeader.svelte';
  import SettingRow from '../../ui/layout/SettingRow.svelte';
  import SettingSection from '../../ui/layout/SettingSection.svelte';
  import FilterEditor from '../../ui/layout/FilterEditor.svelte';
  import BridgeSettings from './BridgeSettings.svelte';
  import TermsInput from '../../ui/components/TermsInput.svelte';
  import TemplateField from '../../ui/layout/TemplateField.svelte';

  const s = $derived(app.settings);
  let resetOpen = $state(false);
  let forgetOpen = $state(false);
  let counts = $state<Counts | null>(null);
  let proxyUrl = $state(app.settings.proxy.url);
  let proxyError = $state<string | null>(null);

  const SAMPLE: NameContext = {
    service: 'patreon',
    creatorId: '2581747',
    creatorName: 'Aster Vale',
    postId: '175580645',
    postTitle: 'Spring Sketchbook: Forest Studies',
    published: '2026-11-17T08:59:08',
    added: '2026-11-17T10:00:00',
    index: 3,
    count: 12,
    fileName: 'forest-morning.png',
    ext: 'png',
    hash: '64bb473acd217b4e30e4c4a65a78b5dd1d0e649f32e8b9c704912165d3625336',
    role: 'attachment',
  };
  const sep = $derived(app.info?.platform === 'windows' ? '\\' : '/');
  const example = $derived(
    `${s.downloadDir}${sep}${buildFilePath(s.folderTemplate, s.fileTemplate, withoutWords(SAMPLE, s.nameRemove)).replaceAll('/', sep)}`,
  );
  const archiveExample = $derived(
    `${s.downloadDir}${sep}${buildArchivePath(s.folderTemplate, s.archiveTemplate, withoutWords(SAMPLE, s.nameRemove)).replaceAll('/', sep)}`,
  );

  const GB = (n: number) =>
    n === 0 ? t('settings.unlimited') : `${(n / 1024).toLocaleString()} GB`;
  const SPEEDS = [0, 512, 1024, 2048, 5120, 10240, 25600];
  const speedLabel = (kb: number) =>
    kb === 0 ? t('settings.unlimited') : `${fmt.bytes(kb * 1024)}/s`;

  onMount(() => {
    void api.counts().then((c) => (counts = c));
    if (router.section) {
      requestAnimationFrame(() =>
        document.getElementById(`s-${router.section}`)?.scrollIntoView({ behavior: 'smooth' }),
      );
    }
  });

  function set<K extends keyof Settings>(key: K, value: Settings[K]) {
    void updateSettings({ [key]: value } as Partial<Settings>)
      .then(() => toast(t('settings.saved'), 'success', { duration: 1400 }))
      .catch((e) => toast(describeError(e), 'error'));
  }

  async function chooseFolder() {
    const picked = await pickFolder(t('settings.downloadDir'), s.downloadDir);
    if (picked) set('downloadDir', picked);
  }

  async function setProxyMode(mode: ProxyMode) {
    set('proxy', { mode, url: proxyUrl });
  }

  async function saveProxyUrl() {
    proxyError = await api.checkProxyUrl(proxyUrl).catch(() => 'invalid');
    if (!proxyError) set('proxy', { mode: 'custom', url: proxyUrl.trim() });
  }

  async function exportBackup() {
    const path = await pickSavePath(
      t('settings.backupSave'),
      `pawchive-downloader-backup-${stamp()}.json`,
      [{ name: 'JSON', extensions: ['json'] }],
    );
    if (!path) return;
    try {
      await api.exportBackup(path);
      toast(t('settings.backupSaved'), 'success');
    } catch (e) {
      toast(describeError(e), 'error');
    }
  }

  async function restoreBackup() {
    const path = await pickFile(t('settings.backupRestore'), [
      { name: 'JSON', extensions: ['json'] },
    ]);
    if (!path) return;
    try {
      const r = await api.importBackup(path);
      toast(t('settings.backupRestored', r), 'success');
    } catch {
      toast(t('settings.backupInvalid'), 'error');
    }
  }

  async function exportFile() {
    const path = await saveTextFile(
      `pawchive-downloader-settings-${stamp()}.json`,
      `${JSON.stringify(s, null, 2)}\n`,
      t('common.export'),
    );
    if (path) toast(t('settings.exported'), 'success');
  }

  async function importFile() {
    try {
      const text = await openTextFile(t('common.import'), ['json']);
      if (!text) return;
      const parsed = JSON.parse(text) as Partial<Settings>;
      // Another kind of file (a backup, a favorites export) is not settings:
      // it would change nothing and still say "imported".
      if (
        !parsed ||
        typeof parsed !== 'object' ||
        Array.isArray(parsed) ||
        !Object.keys(parsed).some((key) => key in app.settings)
      ) {
        throw new Error('invalid');
      }
      await updateSettings(parsed);
      toast(t('settings.imported'), 'success');
    } catch {
      toast(t('settings.importFailed'), 'error');
    }
  }

  async function reset() {
    replaceSettings(await api.resetSettings());
    resetOpen = false;
    proxyUrl = '';
    toast(t('settings.resetDone'), 'success');
  }

  async function forget() {
    await api.forgetKnown();
    forgetOpen = false;
    counts = await api.counts();
    toast(t('settings.forgotten'), 'success');
  }

  const sections = [
    ['appearance', 'palette'],
    ['downloads', 'download'],
    ['naming', 'rename'],
    ['filters', 'filter'],
    ['bridge', 'web'],
    ['zip', 'folderZip'],
    ['network', 'network'],
    ['background', 'tray'],
    ['shortcuts', 'keyboard'],
    ['data', 'database'],
  ] as const;

  // Search: every setting whose text matches stays; a matching section title
  // keeps its whole section. Works on the page itself, so new settings are
  // found without listing them anywhere.
  let query = $state('');
  let nothing = $state(false);

  $effect(() => {
    const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
    const matches = (text: string) => {
      const lower = text.toLowerCase();
      return words.every((w) => lower.includes(w));
    };
    let shown = 0;
    for (const section of document.querySelectorAll<HTMLElement>('[data-section]')) {
      const header = section.querySelector('header')?.textContent ?? '';
      const whole = words.length === 0 || matches(header);
      let any = false;
      for (const child of section.querySelectorAll<HTMLElement>(':scope > .card > *')) {
        const show = whole || matches(child.textContent ?? '');
        child.style.display = show ? '' : 'none';
        any ||= show;
      }
      section.style.display = whole || any ? '' : 'none';
      if (whole || any) shown += 1;
    }
    nothing = words.length > 0 && shown === 0;
  });

  onMount(() => {
    const focusSearch = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') {
        e.preventDefault();
        document.querySelector<HTMLInputElement>('.find input')?.focus();
      }
    };
    window.addEventListener('keydown', focusSearch);
    return () => window.removeEventListener('keydown', focusSearch);
  });

  const SHORTCUTS = [
    ['Ctrl+L', 'shortcut.focusLink'],
    ['Ctrl+V', 'shortcut.paste'],
    ['Ctrl+Shift+B', 'shortcut.batch'],
    ['Ctrl+B', 'shortcut.sidebar'],
    ['Ctrl+1 ... Ctrl+9', 'shortcut.pages'],
    ['Ctrl+,', 'shortcut.settings'],
    ['Ctrl+F', 'shortcut.search'],
    ['Alt+Left / Right', 'shortcut.history'],
    ['Shift+Click', 'shortcut.range'],
    ['Enter', 'shortcut.openPost'],
  ] as const;
</script>

<PageHeader title={t('nav.settings')} description={t('settings.description')} />

<div class="find">
  <TextInput
    label={t('settings.search')}
    placeholder={t('settings.search')}
    icon="search"
    bind:value={query}
    clearable
    onkeydown={(e) => e.key === 'Escape' && (query = '')}
  />
  {#if query.trim() && nothing}
    <span class="faint">{t('settings.searchNone')}</span>
  {/if}
</div>

<nav class="jump" class:hidden={query.trim().length > 0}>
  {#each sections as [id, icon] (id)}
    <a
      href="#settings/{id}"
      onclick={(e) => {
        e.preventDefault();
        document.getElementById(`s-${id}`)?.scrollIntoView({ behavior: 'smooth' });
      }}
    >
      <Icon name={icon} />{t(`settings.${id}`)}
    </a>
  {/each}
</nav>

<SettingSection id="s-appearance" icon="palette" title={t('settings.appearance')}>
  <SettingRow label={t('settings.language')} description={t('settings.languageHint')}>
    <Select
      label={t('settings.language')}
      value={s.language}
      options={[
        { value: 'auto', label: t('settings.languageAuto') },
        ...LOCALES.map((l) => ({ value: l, label: LANGUAGE_NAMES[l] })),
      ]}
      onchange={(v) => set('language', v)}
    />
  </SettingRow>
  <SettingRow label={t('settings.blurPreviews')} description={t('settings.blurPreviewsHint')}>
    <Switch
      label={t('settings.blurPreviews')}
      checked={s.blurPreviews}
      onchange={(v) => set('blurPreviews', v)}
    />
  </SettingRow>
  <SettingRow label={t('settings.theme')} stacked>
    <div class="themes">
      {#each [...THEMES, 'system'] as theme (theme)}
        <button
          type="button"
          class="theme-swatch"
          data-preview={theme}
          class:active={s.theme === theme}
          onclick={() => set('theme', theme as Settings['theme'])}
        >
          <span class="preview"><span></span><span></span><span></span></span>
          <span>{t(`theme.${theme}`)}</span>
        </button>
      {/each}
    </div>
  </SettingRow>
  <SettingRow label={t('settings.accent')}>
    <div class="accents">
      {#each ACCENTS as color (color)}
        <button
          type="button"
          class="accent"
          class:active={s.accent === color}
          style="--c:{color}"
          aria-label={color}
          onclick={() => set('accent', color)}
        ></button>
      {/each}
      <label class="accent custom" style="--c:{s.accent}" data-tip={t('settings.customColor')}>
        <input
          type="color"
          value={s.accent}
          onchange={(e) => set('accent', (e.currentTarget as HTMLInputElement).value)}
        />
        <Icon name="plus" />
      </label>
    </div>
  </SettingRow>
  <SettingRow label={t('settings.motion')} description={t('settings.motionHint')}>
    <Segmented
      label={t('settings.motion')}
      size="sm"
      value={s.reduceMotion}
      onchange={(v) => set('reduceMotion', v)}
      options={[
        { value: 'auto', label: t('motion.auto') },
        { value: 'off', label: t('motion.off') },
        { value: 'on', label: t('motion.on') },
      ]}
    />
  </SettingRow>
</SettingSection>

<SettingSection
  id="s-downloads"
  icon="download"
  title={t('settings.downloads')}
  description={t('settings.downloadsHint')}
>
  <SettingRow label={t('settings.downloadDir')} description={t('settings.downloadDirHint')}>
    <div class="folder-row">
      <code class="path selectable" title={s.downloadDir}>{s.downloadDir}</code>
      <Button size="sm" icon="folder" onclick={chooseFolder}>{t('settings.change')}</Button>
      <Button
        size="sm"
        variant="ghost"
        icon="openFolder"
        onclick={async () => {
          await api.ensureDir(s.downloadDir).catch(() => undefined);
          await api.openPath(s.downloadDir).catch((e) => toast(describeError(e), 'error'));
        }}
      >
        {t('settings.open')}
      </Button>
    </div>
  </SettingRow>
  <SettingRow label={t('settings.mode')} description={t('settings.modeHint')}>
    <Segmented
      label={t('settings.mode')}
      size="sm"
      value={s.downloadMode}
      onchange={(v) => set('downloadMode', v)}
      options={[
        { value: 'individual', label: t('options.individual'), icon: 'layers' },
        { value: 'zip', label: t('options.zip'), icon: 'folderZip' },
        { value: 'thumbnails', label: t('options.thumbnails'), icon: 'thumbnails' },
      ]}
    />
  </SettingRow>
  <SettingRow label={t('settings.fastMode')} description={t('settings.fastModeHint')}>
    <Switch
      label={t('settings.fastMode')}
      checked={s.fastMode}
      onchange={(v) => set('fastMode', v)}
    />
  </SettingRow>
  <SettingRow label={t('settings.concurrency')} description={t('settings.concurrencyHint')}>
    <Segmented
      label={t('settings.concurrency')}
      size="sm"
      value={String(s.concurrency)}
      onchange={(v) => set('concurrency', Number(v))}
      options={['1', '2', '3', '4', '5'].map((n) => ({ value: n, label: n }))}
    />
  </SettingRow>
  <SettingRow label={t('settings.interval')} description={t('settings.intervalHint')}>
    <Select
      label={t('settings.interval')}
      value={s.requestIntervalMs}
      onchange={(v) => set('requestIntervalMs', v)}
      options={[1000, 1500, 2000, 3000, 5000].map((ms) => ({
        value: ms,
        label: t('settings.seconds', { count: ms / 1000 }),
      }))}
    />
  </SettingRow>
  <SettingRow label={t('settings.speedLimit')} description={t('settings.speedLimitHint')}>
    <Select
      label={t('settings.speedLimit')}
      value={s.speedLimitKbps}
      onchange={(v) => set('speedLimitKbps', v)}
      options={SPEEDS.map((kb) => ({ value: kb, label: speedLabel(kb) }))}
    />
  </SettingRow>
  <SettingRow label={t('settings.schedule')} description={t('settings.scheduleHint')}>
    {#if s.scheduleEnabled}
      <span class="clock">
        <TextInput
          type="time"
          label={t('settings.scheduleStart')}
          value={s.scheduleStart}
          onchange={(v) => v && set('scheduleStart', v)}
        />
        <span class="faint">-</span>
        <TextInput
          type="time"
          label={t('settings.scheduleEnd')}
          value={s.scheduleEnd}
          onchange={(v) => v && set('scheduleEnd', v)}
        />
      </span>
    {/if}
    <Switch
      label={t('settings.schedule')}
      checked={s.scheduleEnabled}
      onchange={(v) => set('scheduleEnabled', v)}
    />
  </SettingRow>
  <SettingRow label={t('settings.autoRetry')} description={t('settings.autoRetryHint')}>
    <Select
      label={t('settings.autoRetry')}
      value={s.autoRetryMinutes}
      onchange={(v) => set('autoRetryMinutes', v)}
      options={[0, 15, 30, 60, 180].map((m) => ({
        value: m,
        label: m === 0 ? t('settings.off') : t('settings.afterMinutes', { count: m }),
      }))}
    />
  </SettingRow>
  <SettingRow label={t('settings.keepAwake')} description={t('settings.keepAwakeHint')}>
    <Switch
      label={t('settings.keepAwake')}
      checked={s.keepAwake}
      onchange={(v) => set('keepAwake', v)}
    />
  </SettingRow>
  <SettingRow label={t('settings.minFreeSpace')} description={t('settings.minFreeSpaceHint')}>
    <Select
      label={t('settings.minFreeSpace')}
      value={s.minFreeSpaceMb}
      onchange={(v) => set('minFreeSpaceMb', v)}
      options={[0, 512, 1024, 2048, 5120, 10240].map((mb) => ({
        value: mb,
        label: mb === 0 ? t('settings.off') : fmt.bytes(mb * 1024 * 1024),
      }))}
    />
  </SettingRow>
  <SettingRow label={t('settings.retries')} description={t('settings.retriesHint')}>
    <Select
      label={t('settings.retries')}
      value={s.maxRetries}
      onchange={(v) => set('maxRetries', v)}
      options={[0, 1, 2, 3, 5, 8, 10, 15, 20, 30, 50].map((n) => ({ value: n, label: String(n) }))}
    />
  </SettingRow>
  <SettingRow label={t('settings.conflict')}>
    <Select
      label={t('settings.conflict')}
      value={s.conflictAction}
      onchange={(v) => set('conflictAction', v)}
      options={[
        { value: 'uniquify', label: t('conflict.uniquify') },
        { value: 'overwrite', label: t('conflict.overwrite') },
        { value: 'skip', label: t('conflict.skip') },
      ]}
    />
  </SettingRow>
  <SettingRow label={t('settings.skipDownloaded')} description={t('settings.skipDownloadedHint')}>
    <Switch
      label={t('settings.skipDownloaded')}
      checked={s.skipDownloaded}
      onchange={(v) => set('skipDownloaded', v)}
    />
  </SettingRow>
  <SettingRow label={t('settings.verifyHashes')} description={t('settings.verifyHashesHint')}>
    <Switch
      label={t('settings.verifyHashes')}
      checked={s.verifyHashes}
      onchange={(v) => set('verifyHashes', v)}
    />
  </SettingRow>
  <SettingRow label={t('settings.previewOnly')} description={t('settings.previewOnlyHint')}>
    <Select
      label={t('settings.previewOnly')}
      value={s.previewOnly}
      onchange={(v) => set('previewOnly', v)}
      options={[
        { value: 'relay', label: t('preview.relay') },
        { value: 'thumbnail', label: t('preview.thumbnail') },
        { value: 'skip', label: t('preview.skip') },
      ]}
    />
  </SettingRow>
  <SettingRow label={t('settings.outsideLinks')} description={t('settings.outsideLinksHint')}>
    <Switch
      label={t('settings.outsideLinks')}
      checked={s.downloadOutsideLinks}
      onchange={(v) => set('downloadOutsideLinks', v)}
    />
  </SettingRow>
  <SettingRow label={t('settings.extract')} description={t('settings.extractHint')}>
    <Select
      label={t('settings.extract')}
      value={s.extractArchives}
      onchange={(v) => set('extractArchives', v)}
      options={[
        { value: 'off', label: t('settings.off') },
        { value: 'keep', label: t('settings.extractKeep') },
        { value: 'delete', label: t('settings.extractDelete') },
      ]}
    />
  </SettingRow>
  <SettingRow
    label={t('settings.thumbnailFallback')}
    description={t('settings.thumbnailFallbackHint')}
  >
    <Switch
      label={t('settings.thumbnailFallback')}
      checked={s.thumbnailFallback}
      onchange={(v) => set('thumbnailFallback', v)}
    />
  </SettingRow>
  <SettingRow label={t('settings.content')} description={t('settings.contentHint')}>
    <Select
      label={t('settings.content')}
      value={s.saveContent}
      onchange={(v) => set('saveContent', v)}
      options={[
        { value: 'none', label: t('content.none') },
        { value: 'txt', label: t('content.txt') },
        { value: 'html', label: t('content.html') },
        { value: 'both', label: t('content.both') },
      ]}
    />
  </SettingRow>
  <SettingRow label={t('settings.metadata')} description={t('settings.metadataHint')}>
    <Switch
      label={t('settings.metadata')}
      checked={s.saveMetadata}
      onchange={(v) => set('saveMetadata', v)}
    />
  </SettingRow>
  <SettingRow label={t('settings.links')} description={t('settings.linksHint')}>
    <Switch
      label={t('settings.links')}
      checked={s.saveLinks}
      onchange={(v) => set('saveLinks', v)}
    />
  </SettingRow>
  <SettingRow label={t('settings.comments')} description={t('settings.commentsHint')}>
    <Switch
      label={t('settings.comments')}
      checked={s.saveComments}
      onchange={(v) => set('saveComments', v)}
    />
  </SettingRow>
  <SettingRow label={t('settings.announcements')} description={t('settings.announcementsHint')}>
    <Switch
      label={t('settings.announcements')}
      checked={s.saveAnnouncements}
      onchange={(v) => set('saveAnnouncements', v)}
    />
  </SettingRow>
  <SettingRow label={t('settings.categories')} description={t('settings.categoriesHint')} stacked>
    <div class="chips">
      {#each FILE_CATEGORIES as category (category)}
        <Chip
          label={t(`category.${category}`)}
          active={s.categories[category]}
          onclick={() =>
            set('categories', { ...s.categories, [category]: !s.categories[category] })}
        />
      {/each}
    </div>
  </SettingRow>
</SettingSection>

<SettingSection
  id="s-naming"
  icon="rename"
  title={t('settings.naming')}
  description={t('settings.namingHint')}
>
  <SettingRow label={t('settings.folderTemplate')} stacked>
    <TemplateField
      kind="folder"
      label={t('settings.folderTemplate')}
      value={s.folderTemplate}
      presets={FOLDER_TEMPLATE_PRESETS}
      onsave={(v) => set('folderTemplate', v)}
    />
  </SettingRow>
  <SettingRow label={t('settings.fileTemplate')} stacked>
    <TemplateField
      kind="file"
      label={t('settings.fileTemplate')}
      value={s.fileTemplate}
      presets={FILE_TEMPLATE_PRESETS}
      onsave={(v) => set('fileTemplate', v)}
    />
  </SettingRow>
  <SettingRow label={t('settings.archiveTemplate')} stacked>
    <TemplateField
      kind="file"
      label={t('settings.archiveTemplate')}
      value={s.archiveTemplate}
      presets={{ default: DEFAULT_SETTINGS.archiveTemplate, short: '{post_id}' }}
      onsave={(v) => set('archiveTemplate', v)}
    />
  </SettingRow>
  <SettingRow label={t('settings.nameRemove')} description={t('settings.nameRemoveHint')} stacked>
    <TermsInput
      label={t('settings.nameRemove')}
      placeholder={t('settings.nameRemovePlaceholder')}
      value={s.nameRemove}
      onchange={(v) => set('nameRemove', v)}
    />
  </SettingRow>
  <div class="example">
    <span class="faint">{t('settings.example')}</span>
    <code class="selectable">{example}</code>
    <code class="selectable">{archiveExample}</code>
  </div>
</SettingSection>

<SettingSection
  id="s-filters"
  icon="filter"
  title={t('settings.filters')}
  description={t('settings.filtersHint')}
>
  <div class="filters">
    <FilterEditor value={s.filter} onchange={(v) => set('filter', v)} />
  </div>
</SettingSection>

<SettingSection
  id="s-zip"
  icon="folderZip"
  title={t('settings.zip')}
  description={t('settings.zipHint')}
>
  <SettingRow label={t('settings.zipMax')} description={t('settings.zipMaxHint')}>
    <Select
      label={t('settings.zipMax')}
      value={s.zipMaxSizeMb}
      onchange={(v) => set('zipMaxSizeMb', v)}
      options={[0, 1024, 2048, 4096, 8192, 16384].map((n) => ({ value: n, label: GB(n) }))}
    />
  </SettingRow>
  <SettingRow label={t('settings.zipVideos')} description={t('settings.zipVideosHint')}>
    <Switch
      label={t('settings.zipVideos')}
      checked={s.zipVideosSeparately}
      onchange={(v) => set('zipVideosSeparately', v)}
    />
  </SettingRow>
</SettingSection>

<SettingSection
  id="s-network"
  icon="network"
  title={t('settings.network')}
  description={t('settings.networkHint')}
>
  <SettingRow label={t('settings.proxy')} description={t('settings.proxyHint')}>
    <Select
      label={t('settings.proxy')}
      value={s.proxy.mode}
      onchange={(v) => setProxyMode(v)}
      options={[
        { value: 'system', label: t('proxy.system') },
        { value: 'none', label: t('proxy.none') },
        { value: 'custom', label: t('proxy.custom') },
        { value: 'tor', label: t('proxy.tor') },
      ]}
    />
  </SettingRow>
  {#if s.proxy.mode === 'custom'}
    <SettingRow label={t('settings.proxyUrl')} description={t('settings.proxyUrlHint')} stacked>
      <form
        class="proxy"
        onsubmit={(e) => {
          e.preventDefault();
          void saveProxyUrl();
        }}
      >
        <TextInput
          label={t('settings.proxyUrl')}
          mono
          placeholder="socks5h://127.0.0.1:1080"
          bind:value={proxyUrl}
          invalid={proxyError !== null}
        />
        <Button size="sm" type="submit">{t('common.save')}</Button>
      </form>
      {#if proxyError}<span class="error">{t('proxy.invalid')}</span>{/if}
    </SettingRow>
  {:else if s.proxy.mode === 'tor'}
    <p class="note"><Icon name="shieldLock" /> {t('proxy.torHint')}</p>
  {/if}
  <SettingRow label={t('settings.userAgent')} description={t('settings.userAgentHint')}>
    <code class="ua selectable">{app.info?.userAgent}</code>
  </SettingRow>
</SettingSection>

<SettingSection id="s-background" icon="tray" title={t('settings.background')}>
  <SettingRow label={t('settings.closeToTray')} description={t('settings.closeToTrayHint')}>
    <Switch
      label={t('settings.closeToTray')}
      checked={s.closeToTray}
      onchange={(v) => set('closeToTray', v)}
    />
  </SettingRow>
  <SettingRow label={t('settings.launchAtLogin')} description={t('settings.launchAtLoginHint')}>
    <Switch
      label={t('settings.launchAtLogin')}
      checked={s.launchAtLogin}
      disabled={!inTauri}
      onchange={(v) => set('launchAtLogin', v)}
    />
  </SettingRow>
  <SettingRow label={t('settings.startMinimized')} description={t('settings.startMinimizedHint')}>
    <Switch
      label={t('settings.startMinimized')}
      checked={s.startMinimized}
      onchange={(v) => set('startMinimized', v)}
    />
  </SettingRow>
  <SettingRow label={t('settings.watchClipboard')} description={t('settings.watchClipboardHint')}>
    <Switch
      label={t('settings.watchClipboard')}
      checked={s.watchClipboard}
      onchange={(v) => set('watchClipboard', v)}
    />
  </SettingRow>
  <SettingRow
    label={t('settings.privateNotifications')}
    description={t('settings.privateNotificationsHint')}
  >
    <Switch
      label={t('settings.privateNotifications')}
      checked={s.privateNotifications}
      onchange={(v) => set('privateNotifications', v)}
    />
  </SettingRow>
  <SettingRow label={t('settings.notifications')}>
    <Select
      label={t('settings.notifications')}
      value={s.notifications}
      onchange={(v) => set('notifications', v)}
      options={[
        { value: 'all', label: t('notify.all') },
        { value: 'errors', label: t('notify.errors') },
        { value: 'none', label: t('notify.none') },
      ]}
    />
  </SettingRow>
  <SettingRow label={t('settings.watchInterval')} description={t('settings.watchIntervalHint')}>
    <Select
      label={t('settings.watchInterval')}
      value={s.watchIntervalMinutes}
      onchange={(v) => set('watchIntervalMinutes', v)}
      options={WATCH_INTERVALS.map((n) => ({ value: n, label: t(`interval.${n}`) }))}
    />
  </SettingRow>
  <SettingRow label={t('settings.checkUpdates')} description={t('settings.checkUpdatesHint')}>
    <Switch
      label={t('settings.checkUpdates')}
      checked={s.checkUpdates}
      onchange={(v) => set('checkUpdates', v)}
    />
  </SettingRow>
  <SettingRow label={t('settings.recordHistory')} description={t('settings.recordHistoryHint')}>
    <Switch
      label={t('settings.recordHistory')}
      checked={s.recordHistory}
      onchange={(v) => set('recordHistory', v)}
    />
  </SettingRow>
  <SettingRow label={t('settings.logLevel')}>
    <Segmented
      label={t('settings.logLevel')}
      size="sm"
      value={s.logLevel}
      onchange={(v) => set('logLevel', v)}
      options={[
        { value: 'debug', label: t('logs.debug') },
        { value: 'info', label: t('logs.info') },
        { value: 'warn', label: t('logs.warn') },
        { value: 'error', label: t('logs.error') },
      ]}
    />
  </SettingRow>
</SettingSection>

<SettingSection
  id="s-shortcuts"
  icon="keyboard"
  title={t('settings.shortcuts')}
  description={t('settings.shortcutsHint')}
>
  {#each SHORTCUTS as [keys, label] (label)}
    <SettingRow label={t(label)}>
      <span class="keys">
        {#each keys.split('+') as key, i (i)}<kbd>{key}</kbd>{/each}
      </span>
    </SettingRow>
  {/each}
</SettingSection>

<SettingSection
  id="s-bridge"
  icon="web"
  title={t('settings.bridge')}
  description={t('settings.bridgeHint')}
>
  <BridgeSettings />
</SettingSection>

<SettingSection id="s-data" icon="database" title={t('settings.data')}>
  <SettingRow label={t('settings.exportSettings')} description={t('settings.exportHint')}>
    <Button size="sm" icon="export" onclick={exportFile}>{t('common.export')}</Button>
    <Button size="sm" icon="import" onclick={importFile}>{t('common.import')}</Button>
  </SettingRow>
  <SettingRow label={t('settings.backup')} description={t('settings.backupHint')}>
    <Button size="sm" icon="export" onclick={exportBackup}>{t('settings.backupSave')}</Button>
    <Button size="sm" icon="import" onclick={restoreBackup}>{t('settings.backupRestore')}</Button>
  </SettingRow>
  <SettingRow label={t('settings.dataFolder')} description={t('settings.dataFolderHint')}>
    <Button
      size="sm"
      icon="openFolder"
      disabled={!app.info}
      onclick={() => app.info && api.openPath(app.info.dataDir)}
    >
      {t('settings.open')}
    </Button>
  </SettingRow>
  <SettingRow
    label={t('settings.knownFiles')}
    description={t('settings.knownFilesHint', {
      files: counts?.files ?? 0,
      posts: counts?.posts ?? 0,
    })}
  >
    <Button
      size="sm"
      variant="ghost"
      icon="restore"
      disabled={!counts?.files && !counts?.posts}
      onclick={() => (forgetOpen = true)}
    >
      {t('settings.forget')}
    </Button>
  </SettingRow>
  <SettingRow label={t('settings.reset')} description={t('settings.resetHint')}>
    <Button size="sm" variant="danger" icon="restore" onclick={() => (resetOpen = true)}>
      {t('settings.reset')}
    </Button>
  </SettingRow>
</SettingSection>

<Dialog
  open={resetOpen}
  title={t('settings.resetTitle')}
  confirmLabel={t('settings.reset')}
  danger
  onconfirm={reset}
  oncancel={() => (resetOpen = false)}
>
  <p>{t('settings.resetBody')}</p>
</Dialog>

<Dialog
  open={forgetOpen}
  title={t('settings.forgetTitle')}
  confirmLabel={t('settings.forget')}
  danger
  onconfirm={forget}
  oncancel={() => (forgetOpen = false)}
>
  <p>{t('settings.forgetBody')}</p>
</Dialog>

<style>
  .clock {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .clock :global(.field) {
    width: 110px;
  }
  .filters {
    padding: 16px 18px;
  }
  .folder-row {
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: 560px;
  }
  .path,
  .ua {
    font-size: var(--fs-xs);
    padding: 5px 9px;
    border-radius: var(--radius-sm);
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 320px;
  }
  .proxy {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .proxy :global(> :first-child) {
    flex: 1;
  }
  .error {
    color: var(--danger);
    font-size: var(--fs-xs);
  }
  .note {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 12px 18px;
    border-bottom: 1px solid var(--border);
    font-size: var(--fs-sm);
    color: var(--text-dim);
  }
  .find {
    display: flex;
    align-items: center;
    gap: 12px;
    max-width: 520px;
    /* Room below, so results never sit right against the box when the
       section links hide during a search. */
    margin-bottom: 18px;
  }
  .find :global(.field) {
    flex: 1;
  }
  .jump.hidden {
    display: none;
  }
  .jump {
    position: sticky;
    top: 0;
    z-index: 5;
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 8px 0 14px;
    margin: -12px 0 8px;
    background: linear-gradient(var(--bg) 80%, transparent);
  }
  .jump a {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    border-radius: 99px;
    color: var(--text-dim);
    font-size: var(--fs-sm);
    font-weight: 600;
    border: 1px solid var(--border);
    background: var(--bg-elev);
    text-decoration: none;
    transition:
      color var(--dur-fast) var(--ease),
      border-color var(--dur-fast) var(--ease);
  }
  .jump a:hover {
    color: var(--text);
    border-color: var(--border-strong);
  }
  .themes {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(118px, 1fr));
    gap: 10px;
  }
  .theme-swatch {
    display: grid;
    gap: 8px;
    padding: 8px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: var(--bg-elev-2);
    color: var(--text-dim);
    font-size: var(--fs-sm);
    font-weight: 600;
    text-align: left;
    transition:
      border-color var(--dur-fast) var(--ease),
      transform var(--dur) var(--ease-spring);
  }
  .theme-swatch:hover {
    border-color: var(--border-strong);
    transform: translateY(-1px);
  }
  .theme-swatch.active {
    border-color: var(--accent);
    color: var(--text);
    box-shadow: 0 0 0 1px var(--accent) inset;
  }
  .preview {
    display: grid;
    grid-template-columns: 1fr 2fr;
    grid-template-rows: 1fr 1fr;
    gap: 4px;
    height: 48px;
    padding: 5px;
    border-radius: 7px;
    background: var(--p-bg);
    border: 1px solid var(--p-border);
  }
  .preview span {
    border-radius: 4px;
    background: var(--p-elev);
  }
  .preview span:first-child {
    grid-row: span 2;
  }
  .preview span:last-child {
    background: var(--accent);
    opacity: 0.85;
  }
  [data-preview='midnight'] {
    --p-bg: #0c0e13;
    --p-elev: #1f2430;
    --p-border: #232937;
  }
  [data-preview='graphite'] {
    --p-bg: #111;
    --p-elev: #262626;
    --p-border: #2b2b2b;
  }
  [data-preview='plum'] {
    --p-bg: #120d18;
    --p-elev: #282033;
    --p-border: #2e2439;
  }
  [data-preview='forest'] {
    --p-bg: #0b110e;
    --p-elev: #1c2a23;
    --p-border: #22302a;
  }
  [data-preview='ember'] {
    --p-bg: #130e0c;
    --p-elev: #2b201a;
    --p-border: #33261f;
  }
  [data-preview='light'] {
    --p-bg: #f6f7fb;
    --p-elev: #e8ebf2;
    --p-border: #e1e5ee;
  }
  [data-preview='system'] {
    --p-bg: linear-gradient(135deg, #0c0e13 50%, #f6f7fb 50%);
    --p-elev: #8888;
    --p-border: #303849;
  }
  .accents {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .accent {
    position: relative;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    border: 0;
    background: var(--c);
    padding: 0;
    box-shadow: 0 0 0 2px var(--bg-elev);
    transition:
      transform var(--dur) var(--ease-spring),
      box-shadow var(--dur-fast) var(--ease);
  }
  .accent:hover {
    transform: scale(1.12);
  }
  .accent.active {
    box-shadow:
      0 0 0 2px var(--bg-elev),
      0 0 0 4px var(--c);
  }
  .custom {
    display: grid;
    place-items: center;
    background: var(--bg-elev-3);
    color: var(--text-dim);
    cursor: pointer;
    border: 1px dashed var(--border-strong);
  }
  .custom input {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .example {
    display: grid;
    gap: 6px;
    padding: 14px 18px;
    background: var(--bg);
    font-size: var(--fs-sm);
  }
  code {
    font-family: var(--font);
    color: var(--accent-strong);
    word-break: break-all;
  }
  .keys {
    display: inline-flex;
    gap: 4px;
  }
  kbd {
    font-family: var(--font);
    font-size: var(--fs-xs);
    font-weight: 700;
    padding: 2px 7px;
    border-radius: 6px;
    background: var(--bg-elev-3);
    border: 1px solid var(--border-strong);
    border-bottom-width: 2px;
  }
</style>
