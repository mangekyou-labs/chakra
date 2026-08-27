import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createQuoteScheduler } from './quote-scheduler';

describe('quote scheduler (250 ms debounce / 5 s refresh)', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('debounces burst of schedule() calls to a single fetch after 250 ms', async () => {
    const fetchMock = vi.fn().mockResolvedValue(undefined);
    const s = createQuoteScheduler({ debounceMs: 250, refreshMs: 5000, fetch: fetchMock });

    s.schedule();
    await vi.advanceTimersByTimeAsync(100);
    s.schedule();
    await vi.advanceTimersByTimeAsync(100);
    s.schedule();
    await vi.advanceTimersByTimeAsync(250);

    expect(fetchMock).toHaveBeenCalledTimes(1);
    s.stop();
  });

  it('the 5 s refresh fires and does not overlap an in-flight fetch', async () => {
    let resolveFetch: () => void = () => {};
    const fetchMock = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveFetch = resolve;
        }),
    );
    const s = createQuoteScheduler({ debounceMs: 250, refreshMs: 5000, fetch: fetchMock });

    s.schedule();
    await vi.advanceTimersByTimeAsync(250);
    expect(fetchMock).toHaveBeenCalledTimes(1);

    // Refresh fires at 5 s while the first fetch is still in flight → skipped.
    await vi.advanceTimersByTimeAsync(5000);
    expect(fetchMock).toHaveBeenCalledTimes(1);

    resolveFetch();
    await vi.advanceTimersByTimeAsync(0);
    s.stop();
  });
});
