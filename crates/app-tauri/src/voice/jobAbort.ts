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
}

/** waitForJob rejects with Error('aborted') when its signal fires. */
export function isAbortError(err: unknown): boolean {
  return err instanceof Error && err.message === 'aborted';
}
