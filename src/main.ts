import './app.css';
import { mount } from 'svelte';
import App from './App.svelte';
import { api } from './lib/api';

const target = document.getElementById('app');
if (!target) throw new Error('Missing #app');

// The window starts hidden so it never flashes white; show it once the
// fonts are in and the first frame is painted.
void document.fonts.ready.then(() => {
  requestAnimationFrame(() =>
    requestAnimationFrame(() => void api.uiReady().catch(() => undefined)),
  );
});

// Keep the native context menu out of the interface, except in text fields.
window.addEventListener('contextmenu', (event) => {
  const el = event.target as HTMLElement | null;
  if (el?.closest('input, textarea, .selectable')) return;
  if (import.meta.env.DEV) return;
  event.preventDefault();
});

export default mount(App, { target });
