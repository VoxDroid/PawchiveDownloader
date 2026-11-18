import { describe, expect, it } from 'vitest';
import { errorText } from '../../src/lib/api';
import {
  formatBytes,
  formatDate,
  formatDuration,
  formatRelative,
  translate,
} from '../../src/lib/i18n';
import { createMockBackend } from '../../src/lib/mock';
import { CATEGORY_ICON, htmlToText, parseApiDate } from '../../src/lib/text';
import { DEFAULT_SETTINGS, LOCALES, resolveLocale } from '../../src/lib/types';
import type { PostPage, PostView, Settings } from '../../src/lib/types';
import { pageUrls } from '../../src/lib/urls';

describe('text helpers', () => {
  it('turns post HTML into plain text', () => {
    expect(
      htmlToText('<p>One &amp; two</p><ul><li>a</li></ul><a href="https://x.y">site</a><br>end'),
    ).toBe('One & two\n- a\nsite (https://x.y)\nend');
    expect(htmlToText('&#72;i&nbsp;&lt;3')).toBe('Hi <3');
  });

  it('parses API dates as UTC', () => {
    expect(parseApiDate('2026-11-17T08:59:08')).toBe(Date.UTC(2026, 10, 17, 8, 59, 8));
    expect(parseApiDate('2026-11-17T08:59:08+02:00')).toBe(Date.UTC(2026, 10, 17, 6, 59, 8));
    expect(parseApiDate(null)).toBeNull();
    expect(parseApiDate('nope')).toBeNull();
    expect(CATEGORY_ICON.other).toBe('file');
  });

  it('builds page links with encoding', () => {
    expect(pageUrls.post('patreon', '1', '2')).toBe('https://pawchive.pw/patreon/user/1/post/2');
    expect(pageUrls.creator('fanbox', 'a b')).toBe('https://pawchive.pw/fanbox/user/a%20b');
    expect(pageUrls.icon('patreon', '1')).toBe('https://pawchive.pw/icons/patreon/1');
  });

  it('reads errors from the Rust side', () => {
    expect(errorText('not-found')).toBe('not-found');
    expect(errorText(new Error('boom'))).toBe('boom');
    expect(errorText(42)).toBe('42');
  });
});

describe('locales and formatting', () => {
  it('maps system languages to locales', () => {
    expect(resolveLocale('auto', 'pt-BR')).toBe('pt_BR');
    expect(resolveLocale('auto', 'zh-Hans-CN')).toBe('zh_CN');
    expect(resolveLocale('auto', 'de-AT')).toBe('de');
    expect(resolveLocale('auto', 'xx')).toBe('en');
    expect(resolveLocale('ja', 'en-US')).toBe('ja');
  });

  it('translates with plurals and placeholders in every language', () => {
    for (const locale of LOCALES) {
      expect(translate(locale, 'batch.posts', { count: 2 })).toContain('2');
      expect(translate(locale, 'nav.browse')).not.toBe('nav.browse');
      expect(translate(locale, 'help.start.body').length).toBeGreaterThan(80);
    }
    expect(translate('en', 'batch.posts', { count: 1 })).toBe('1 post');
    expect(translate('ru', 'batch.posts', { count: 5 })).toBe('5 публикаций');
    expect(translate('en', 'missing.key')).toBe('missing.key');
  });

  it('formats sizes, durations and dates', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(1536)).toBe('1.50 KB');
    expect(formatDuration(61_000)).toMatch(/1m\s*1s/);
    expect(formatRelative(Date.now() - 3_600_000)).toMatch(/hour/);
    expect(formatDate('2026-01-02T00:00:00')).toContain('2026');
    expect(formatDate(null)).toBe('');
  });
});

describe('demo backend', () => {
  it('answers like the Rust side', async () => {
    const backend = createMockBackend();
    const settings = await backend.invoke<Settings>('get_settings');
    expect(settings.concurrency).toBe(DEFAULT_SETTINGS.concurrency);
    const events: Settings[] = [];
    await backend.listen<Settings>('settings', (s) => events.push(s));
    const next = await backend.invoke<Settings>('update_settings', { patch: { theme: 'plum' } });
    expect(next.theme).toBe('plum');
    expect(events[0]?.theme).toBe('plum');
    const page = await backend.invoke<PostPage>('get_creator_posts', {
      creatorId: '48151623',
      offset: 0,
      query: null,
    });
    expect(page.posts).toHaveLength(50);
    const view = await backend.invoke<PostView>('get_post', {
      creatorId: page.posts[0]?.creatorId,
      postId: page.posts[0]?.id,
    });
    expect(view.post.files.length).toBeGreaterThan(0);
    await expect(backend.invoke('nope')).rejects.toThrow(/no "nope"/);
  });
});
