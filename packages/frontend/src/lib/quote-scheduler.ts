/**
 * Quote request scheduler: debounced input changes + periodic refresh that
 * never overlaps an in-flight fetch (T6.2: 250 ms / 5 s).
 */
export interface QuoteSchedulerOptions {
  debounceMs?: number;
  refreshMs?: number;
  fetch: () => Promise<unknown>;
}

export function createQuoteScheduler({
  debounceMs = 250,
  refreshMs = 5000,
  fetch,
}: QuoteSchedulerOptions): {
  schedule: () => void;
  stop: () => void;
} {
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let refreshTimer: ReturnType<typeof setInterval> | null = null;
  let inFlight: Promise<unknown> | null = null;

  const runFetch = () => {
    if (inFlight) return;
    inFlight = fetch().finally(() => {
      inFlight = null;
    });
  };

  const schedule = () => {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(runFetch, debounceMs);
  };

  // Refresh every `refreshMs`; skipped entirely while a fetch is in flight.
  refreshTimer = setInterval(runFetch, refreshMs);

  return {
    schedule,
    stop: () => {
      if (debounceTimer) clearTimeout(debounceTimer);
      if (refreshTimer) clearInterval(refreshTimer);
    },
  };
}
