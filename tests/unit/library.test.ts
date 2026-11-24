import { render, screen, waitFor } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { describe, expect, it, vi } from 'vitest';
import { setBackend } from '../../src/lib/api';
import type { Gallery, JobSummary } from '../../src/lib/types';
import { app } from '../../src/ui/state/app.svelte';
import LibraryPage from '../../src/features/library/LibraryPage.svelte';

const empty: Gallery = { creators: [], posts: [], images: 0, videos: 0, bytes: 0, missing: 0 };

describe('Library', () => {
  it('reads the library again when a download finishes', async () => {
    const calls = vi.fn(async (command: string) => (command === 'library_media' ? empty : null));
    setBackend({ invoke: calls as never, listen: async () => () => undefined });
    render(LibraryPage);
    await screen.findByText('Your library is empty');
    const reads = () => calls.mock.calls.filter(([c]) => c === 'library_media').length;
    expect(reads()).toBe(1);
    app.jobs = [{ id: 'j', status: 'done', finishedAt: 5 } as JobSummary];
    flushSync();
    await waitFor(() => expect(reads()).toBe(2));
    // A running download changes nothing.
    app.jobs = [...app.jobs, { id: 'k', status: 'running', finishedAt: null } as JobSummary];
    flushSync();
    await new Promise((r) => setTimeout(r, 50));
    expect(reads()).toBe(2);
    app.jobs = [];
  });
});
