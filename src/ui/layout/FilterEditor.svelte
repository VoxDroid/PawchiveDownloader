<script lang="ts">
  import type { PostFilter } from '../../lib/types';
  import { t } from '../state/app.svelte';
  import Segmented from '../components/Segmented.svelte';
  import Select from '../components/Select.svelte';
  import TermsInput from '../components/TermsInput.svelte';

  interface Props {
    value: PostFilter;
    onchange: (value: PostFilter) => void;
    /** Show the post rules (keywords, skip words, limit). */
    posts?: boolean;
  }

  let { value, onchange, posts = true }: Props = $props();

  function patch(change: Partial<PostFilter>) {
    onchange({ ...value, ...change });
  }

  const LIMITS = [0, 1, 5, 10, 25, 50, 100, 250, 500];
</script>

<div class="editor">
  {#if posts}
    <div class="field">
      <span class="label">{t('filter.keywords')}</span>
      <TermsInput
        label={t('filter.keywords')}
        placeholder={t('filter.keywordsHint')}
        value={value.keywords}
        onchange={(keywords) => patch({ keywords })}
      />
    </div>
    <div class="field">
      <span class="label">{t('filter.skipWords')}</span>
      <TermsInput
        label={t('filter.skipWords')}
        placeholder={t('filter.skipWordsHint')}
        value={value.skipWords}
        onchange={(skipWords) => patch({ skipWords })}
      />
    </div>
    <div class="row">
      <div class="field">
        <span class="label">{t('filter.scope')}</span>
        <Segmented
          size="sm"
          label={t('filter.scope')}
          value={value.scope}
          onchange={(scope) => patch({ scope })}
          options={[
            { value: 'title', label: t('filter.scopeTitle'), icon: 'tag' },
            { value: 'all', label: t('filter.scopeAll'), icon: 'text' },
          ]}
        />
      </div>
      <div class="field">
        <span class="label">{t('filter.maxPosts')}</span>
        <Select
          label={t('filter.maxPosts')}
          value={value.maxPosts}
          onchange={(maxPosts) => patch({ maxPosts })}
          options={(LIMITS.includes(value.maxPosts)
            ? LIMITS
            : [...LIMITS, value.maxPosts].sort((a, b) => a - b)
          ).map((n) => ({
            value: n,
            label: n === 0 ? t('filter.noLimit') : t('filter.newest', { count: n }),
          }))}
        />
      </div>
    </div>
  {/if}
  <div class="field">
    <span class="label">{t('filter.extensions')}</span>
    <TermsInput
      extensions
      label={t('filter.extensions')}
      placeholder={t('filter.extensionsHint')}
      value={value.extensions}
      onchange={(extensions) => patch({ extensions })}
    />
  </div>
  <div class="field">
    <span class="label">{t('filter.skipExtensions')}</span>
    <TermsInput
      extensions
      label={t('filter.skipExtensions')}
      placeholder={t('filter.skipExtensionsHint')}
      value={value.skipExtensions}
      onchange={(skipExtensions) => patch({ skipExtensions })}
    />
  </div>
  <div class="field">
    <span class="label">{t('filter.fileSkipWords')}</span>
    <TermsInput
      label={t('filter.fileSkipWords')}
      placeholder={t('filter.fileSkipWordsHint')}
      value={value.fileSkipWords}
      onchange={(fileSkipWords) => patch({ fileSkipWords })}
    />
  </div>
  <p class="hint">{t('filter.hint')}</p>
</div>

<style>
  .editor {
    display: grid;
    gap: 14px;
    min-width: 0;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 14px 24px;
  }
  .field {
    display: grid;
    gap: 6px;
    min-width: 0;
  }
  .label {
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--text-dim);
  }
  .hint {
    margin: 0;
    font-size: var(--fs-xs);
    color: var(--text-faint);
  }
</style>
