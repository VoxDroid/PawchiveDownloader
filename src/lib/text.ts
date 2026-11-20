import type { FileCategory } from './types';

const NAMED: Record<string, string> = {
  amp: '&',
  quot: '"',
  apos: "'",
  lt: '<',
  gt: '>',
  nbsp: ' ',
};

const ENTITY_RE = /&(?:#(\d{1,7})|#x([0-9a-f]{1,6})|([a-z]+));/gi;

function fromCodePoint(code: number, original: string): string {
  // Out of range and lone surrogate code points stay as written.
  if (!Number.isInteger(code) || code < 1 || code > 0x10ffff) return original;
  if (code >= 0xd800 && code <= 0xdfff) return original;
  return String.fromCodePoint(code);
}

/** Decodes HTML entities in one pass, so `&amp;lt;` comes out as `&lt;`. */
export function decodeEntities(value: string): string {
  return value.replace(ENTITY_RE, (match, dec?: string, hex?: string, name?: string) => {
    if (dec) return fromCodePoint(Number.parseInt(dec, 10), match);
    if (hex) return fromCodePoint(Number.parseInt(hex, 16), match);
    return NAMED[(name as string).toLowerCase()] ?? match;
  });
}

/** Removes every tag, repeating until none is left, so `<<b>i>` cannot rebuild one. */
export function stripTags(html: string): string {
  let text = html;
  for (;;) {
    const next = text.replace(/<[a-z!/?][^<>]*>/gi, '');
    if (next === text) return text;
    text = next;
  }
}

/** Post body HTML as readable plain text (the interface never renders HTML). */
export function htmlToText(html: string): string {
  const marked = html
    .replace(/\r\n?/g, '\n')
    .replace(/<\s*br\s*\/?>/gi, '\n')
    .replace(/<\s*\/\s*(p|div|h[1-6]|li|blockquote|pre|tr)\s*>/gi, '\n')
    .replace(/<\s*li\b[^>]*>/gi, '- ')
    .replace(/<a\b[^>]*?href\s*=\s*(["'])(.*?)\1[^>]*>(.*?)<\/a>/gi, (_m, _q, href, text) =>
      text && text !== href ? `${text} (${href})` : href,
    );
  return decodeEntities(stripTags(marked))
    .split('\n')
    .map((line) => line.trimEnd())
    .join('\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

export const CATEGORY_ICON: Record<
  FileCategory,
  'image' | 'video' | 'audio' | 'archive' | 'document' | 'text' | 'file'
> = {
  image: 'image',
  video: 'video',
  audio: 'audio',
  archive: 'archive',
  document: 'document',
  text: 'text',
  other: 'file',
};

/** Pawchive serves API dates without a zone; they are UTC. */
export function parseApiDate(value: string | null): number | null {
  if (!value) return null;
  const text = /[zZ]|[+-]\d\d:?\d\d$/.test(value) ? value : `${value.replace(' ', 'T')}Z`;
  const time = Date.parse(text);
  return Number.isNaN(time) ? null : time;
}
