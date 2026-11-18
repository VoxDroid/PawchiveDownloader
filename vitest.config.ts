import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { svelteTesting } from '@testing-library/svelte/vite';

export default defineConfig({
  plugins: [svelte(), svelteTesting()],
  test: {
    globals: true,
    environment: 'jsdom',
    include: ['tests/unit/**/*.test.ts'],
    setupFiles: ['tests/setup.ts'],
    restoreMocks: true,
    coverage: {
      provider: 'v8',
      include: ['src/lib/**/*.ts'],
      // The demo backend and thin Tauri wrappers are exercised by the UI tests.
      exclude: ['src/lib/**/*.d.ts', 'src/lib/mock.ts', 'src/lib/files.ts'],
      reporter: ['text', 'text-summary', 'lcov', 'html'],
    },
  },
});
