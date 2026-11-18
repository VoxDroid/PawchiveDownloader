import type { FileCategory } from './types';

function decodeEntities(value: string): string {
  return value
    .replace(/&quot;/g, '"')
    .replace(/&#39;|&#x27;/g, "'")
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&#(\d+);/g, (_m, code: string) => String.fromCodePoint(Number(code)))
    .replace(/&amp;/g, '&');
}

/** Post body HTML as readable plain text (the interface never renders HTML). */
export function htmlToText(html: string): string {
  return decodeEntities(
    html
      .replace(/\r\n?/g, '\n')
      .replace(/<\s*br\s*\/?>/gi, '\n')
      .replace(/<\s*\/\s*(p|div|h[1-6]|li|blockquote|pre|tr)\s*>/gi, '\n')
      .replace(/<\s*li\b[^>]*>/gi, '- ')
      .replace(/<a\b[^>]*?href\s*=\s*(["'])(.*?)\1[^>]*>(.*?)<\/a>/gi, (_m, _q, href, text) =>
        text && text !== href ? `${text} (${href})` : href,
      )
      .replace(/<[^>]+>/g, '')
      .replace(/&nbsp;/g, ' '),
  )
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
