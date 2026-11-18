import { api, call, errorText, inTauri, pickFile, pickSavePath } from './api';

/** `2026-11-18_0930` for file names. */
export function stamp(date = new Date()): string {
  const p = (n: number) => String(n).padStart(2, '0');
  return `${date.getFullYear()}-${p(date.getMonth() + 1)}-${p(date.getDate())}_${p(date.getHours())}${p(date.getMinutes())}`;
}

const FILTERS: Record<string, { name: string; extensions: string[] }> = {
  csv: { name: 'CSV', extensions: ['csv'] },
  json: { name: 'JSON', extensions: ['json'] },
  txt: { name: 'Text', extensions: ['txt'] },
  log: { name: 'Log', extensions: ['log', 'txt'] },
};

/**
 * Asks where to save and writes the text there. Returns the path, or null
 * when the user cancelled. In a browser (demo mode) the file is downloaded.
 */
export async function saveTextFile(
  name: string,
  text: string,
  title: string,
): Promise<string | null> {
  if (!inTauri) {
    const url = URL.createObjectURL(new Blob([text], { type: 'text/plain' }));
    const a = Object.assign(document.createElement('a'), { href: url, download: name });
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    return name;
  }
  const ext = name.split('.').pop() ?? 'txt';
  const path = await pickSavePath(title, name, [FILTERS[ext] ?? FILTERS.txt!]);
  if (!path) return null;
  await call<void>('write_text_file', { path, text });
  return path;
}

/** Asks for a text file and returns its content, or null when cancelled. */
export async function openTextFile(title: string, extensions = ['txt']): Promise<string | null> {
  const path = await pickFile(title, [{ name: 'Text', extensions }]);
  if (!path) return null;
  try {
    return await api.readTextFile(path);
  } catch (e) {
    throw new Error(errorText(e), { cause: e });
  }
}
