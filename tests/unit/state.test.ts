import { describe, expect, it } from 'vitest';
import {
  batch,
  clearRecent,
  loadRecent,
  rememberRecent,
} from '../../src/features/browse/batch.svelte';
import type { JobSummary } from '../../src/lib/types';
import { isFinished, overallProgress } from '../../src/ui/state/app.svelte';
import { dismissToast, toast, toasts } from '../../src/ui/state/toasts.svelte';

const job = (status: JobSummary['status'], done: number, total: number, speed = 0) =>
  ({
    status,
    speed,
    stats: {
      total,
      done,
      failed: 0,
      skipped: 0,
      cancelled: 0,
      active: 0,
      bytes: 0,
      totalBytes: 0,
      partial: 0,
    },
  }) as JobSummary;

describe('app state', () => {
  it('sums progress over running jobs only', () => {
    const p = overallProgress([
      job('running', 1, 4, 100),
      job('queued', 0, 4, 50),
      job('done', 9, 9),
    ]);
    expect(p.active).toBe(2);
    expect(p.ratio).toBe(1 / 8);
    expect(p.speed).toBe(150);
    expect(isFinished(job('cancelled', 0, 1))).toBe(true);
    expect(isFinished(job('paused', 0, 1))).toBe(false);
  });

  it('keeps at most four toasts', () => {
    for (let i = 0; i < 6; i++) toast(`t${i}`, 'info', { duration: 0 });
    expect(toasts).toHaveLength(4);
    dismissToast(toasts[0]!.id);
    expect(toasts).toHaveLength(3);
  });

  it('remembers recent links and opens the batch dialog', () => {
    clearRecent();
    rememberRecent({ url: 'a', title: 'A', kind: 'post', at: 1 });
    rememberRecent({ url: 'b', title: 'B', kind: 'creator', at: 2 });
    rememberRecent({ url: 'a', title: 'A again', kind: 'post', at: 3 });
    expect(loadRecent().map((r) => r.title)).toEqual(['A again', 'B']);
    batch.show('x');
    expect(batch.open).toBe(true);
    expect(batch.text).toBe('x');
    batch.close();
    expect(batch.open).toBe(false);
  });
});
