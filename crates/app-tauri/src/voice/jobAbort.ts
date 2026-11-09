/**
 * One AbortController per job a hook follows; abortAll() on unmount stops every waitForJob poller,
 * so no polling or state update outlives the card.
 */
export class JobAbortScope {
  private readonly live = new Set<AbortController>();
  private closed = false;

  signal(): AbortSignal {
    const c = new AbortController();
    if (this.closed) c.abort();
    else this.live.add(c);
    return c.signal;
  }

  /** The job finished: its controller is no longer tracked. */
  release(signal: AbortSignal): void {
    for (const c of this.live) if (c.signal === signal) this.live.delete(c);
  }

  abortAll(): void {
    this.closed = true;
    for (const c of this.live) c.abort();
    this.live.clear();
  }

  isClosed(): boolean {
    return this.closed;
  }
}

/** waitForJob rejects with Error('aborted') when its signal fires. */
export function isAbortError(err: unknown): boolean {
  return err instanceof Error && err.message === 'aborted';
}

/**
 * The scope to use now. abortAll() closes a scope for good, and React StrictMode (mount, unmount,
 * mount) runs the unmount cleanup on a live card: a closed scope would hand out pre-aborted signals
 * and every poller would die silently. A closed scope is therefore replaced by a fresh one.
 */
export function ensureOpenScope(current: JobAbortScope | null): JobAbortScope {
  return current === null || current.isClosed() ? new JobAbortScope() : current;
}
