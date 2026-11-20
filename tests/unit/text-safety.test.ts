import { describe, expect, it } from 'vitest';
import { decodeEntities, htmlToText, stripTags } from '../../src/lib/text';

describe('decodeEntities', () => {
  it('decodes once, so an escaped entity stays an entity', () => {
    expect(decodeEntities('&amp;lt;b&amp;gt;')).toBe('&lt;b&gt;');
    expect(decodeEntities('&#38;amp;')).toBe('&amp;');
  });

  it('leaves invalid and unknown entities as written instead of throwing', () => {
    for (const raw of ['&#99999999;', '&#xFFFFFF;', '&#0;', '&#xD800;', '&bogus;', '& alone']) {
      expect(decodeEntities(raw)).toBe(raw);
    }
    expect(decodeEntities('&#233;&#xE9;&#x1F43E;')).toBe('éé🐾');
  });
});

describe('htmlToText', () => {
  it('never rebuilds a tag and keeps escaped markup as text', () => {
    expect(stripTags('<<b>script>alert(1)<</b>/script>')).not.toMatch(/<script/i);
    expect(stripTags('1 < 2 and 3 > 2')).toBe('1 < 2 and 3 > 2');
    expect(htmlToText('<p>&amp;lt;script&amp;gt;</p>')).toBe('&lt;script&gt;');
    expect(htmlToText('<p>&lt;3 you&#99999999;</p>')).toBe('<3 you&#99999999;');
  });
});
