import { convertFileSrc } from '@tauri-apps/api/core';
import { inTauri } from './api';
import type { MediaFile } from './types';

/**
 * Where the window loads a Library file from: the app's `pdl-media`
 * protocol, which serves recorded files by hash only (see gallery.rs).
 */
export function mediaSrc(file: MediaFile, thumb = false): string {
  if (!inTauri) return (thumb ? file.demo?.thumb : file.demo?.src) ?? '';
  const url = convertFileSrc(file.hash, 'pdl-media');
  return thumb ? `${url}?thumb=1` : url;
}
