import { errorText } from '../../lib/api';
import { t } from './app.svelte';

/**
 * Turns the short error codes and messages the Rust side sends into text for
 * people. Unknown messages are shown as they are.
 */
export function describeError(error: unknown): string {
  const raw = errorText(error).trim();
  if (raw === 'not-found') return t('error.notFound');
  if (raw.startsWith('folder-unwritable')) return t('error.folderUnwritable');
  if (raw === 'no connection') return t('error.offline');
  if (raw === 'got a web page instead of the file') return t('error.notAFile');
  if (raw === 'the file does not match its checksum') return t('error.checksum');
  if (raw.startsWith('internal error')) return t('error.internal');
  if (raw === 'timed out') return t('error.timeout');
  if (raw === 'could not connect' || raw === 'connection dropped') return t('error.connection');
  const http = /^HTTP (\d{3})$/.exec(raw);
  if (http) {
    const status = Number(http[1]);
    if (status === 404 || status === 410) return t('error.http404', { status });
    if (status === 401 || status === 403) return t('error.http403', { status });
    if (status === 429) return t('error.http429', { status });
    if (status >= 500) return t('error.http5xx', { status });
    return t('error.http', { status });
  }
  return raw;
}
