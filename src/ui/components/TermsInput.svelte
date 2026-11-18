<script lang="ts">
  import { t } from '../state/app.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    /** The terms; replaced (not mutated) on every change. */
    value: string[];
    label: string;
    placeholder?: string;
    /** Strip a leading dot and lower-case, for file extensions. */
    extensions?: boolean;
    max?: number;
    onchange: (value: string[]) => void;
  }

  let { value, label, placeholder, extensions = false, max = 100, onchange }: Props = $props();

  let draft = $state('');
  let input = $state<HTMLInputElement>();

  function normalize(term: string): string {
    const trimmed = term.trim().slice(0, 200);
    return extensions ? trimmed.replace(/^\.+/, '').toLowerCase().slice(0, 16) : trimmed;
  }

  /** Adds what was typed; commas separate terms, except inside a /regex/. */
  function commit(text = draft) {
    const parts = /^\s*\/.*\/\s*$/.test(text) ? [text] : text.split(',');
    const next = [...value];
    for (const part of parts) {
      const term = normalize(part);
      if (!term || next.length >= max) continue;
      if (next.some((v) => v.toLowerCase() === term.toLowerCase())) continue;
      next.push(term);
    }
    draft = '';
    if (next.length !== value.length) onchange(next);
  }

  function remove(index: number) {
    onchange(value.filter((_, i) => i !== index));
    input?.focus();
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' || (event.key === ',' && !draft.trimStart().startsWith('/'))) {
      event.preventDefault();
      commit();
    } else if (event.key === 'Backspace' && !draft && value.length) {
      remove(value.length - 1);
    }
  }
</script>

<!-- Clicking the empty space focuses the input; the keyboard reaches it directly. -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<div
  class="terms"
  role="group"
  aria-label={label}
  onclick={(e) => e.target === e.currentTarget && input?.focus()}
>
  {#each value as term, i (term)}
    <span class="term" class:regex={term.length > 2 && term.startsWith('/') && term.endsWith('/')}>
      <span class="truncate">{extensions ? `.${term}` : term}</span>
      <button type="button" aria-label={t('filter.removeTerm', { term })} onclick={() => remove(i)}>
        <Icon name="close" />
      </button>
    </span>
  {/each}
  <input
    bind:this={input}
    bind:value={draft}
    {placeholder}
    aria-label={label}
    spellcheck="false"
    autocomplete="off"
    {onkeydown}
    onblur={() => draft.trim() && commit()}
    onpaste={(e) => {
      const text = e.clipboardData?.getData('text') ?? '';
      if (text.includes(',') || text.includes('\n')) {
        e.preventDefault();
        commit(text.replaceAll('\n', ','));
      }
    }}
  />
</div>

<style>
  .terms {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    min-height: 36px;
    padding: 4px 8px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: var(--bg-elev-2);
    cursor: text;
    transition: border-color var(--dur-fast) var(--ease);
  }
  .terms:focus-within {
    border-color: var(--accent);
    box-shadow: var(--focus-ring);
  }
  .term {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    max-width: 260px;
    height: 24px;
    padding: 0 2px 0 9px;
    border-radius: 99px;
    background: var(--accent-soft);
    color: var(--text);
    font-size: var(--fs-sm);
  }
  .term.regex {
    font-variant-ligatures: none;
    background: var(--bg-elev-3);
  }
  .term button {
    display: inline-grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border: 0;
    border-radius: 99px;
    background: transparent;
    color: var(--text-dim);
  }
  .term button:hover {
    color: var(--text);
    background: color-mix(in srgb, var(--text) 12%, transparent);
  }
  input {
    flex: 1;
    min-width: 120px;
    height: 26px;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: var(--fs-sm);
  }
  input::placeholder {
    color: var(--text-faint);
  }
</style>
