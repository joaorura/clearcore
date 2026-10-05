import { useEffect, useRef, useState } from 'react';

/**
 * An in-app confirmation (no window.confirm): `ask` resolves when the user answers; a pending
 * question is answered "no" if another one replaces it or the card unmounts.
 */
export function useConfirmDialog<T>(): {
  pending: T | null;
  ask: (info: T) => Promise<boolean>;
  answer: (ok: boolean) => void;
} {
  const [pending, setPending] = useState<T | null>(null);
  const resolverRef = useRef<((ok: boolean) => void) | null>(null);

  useEffect(() => () => {
    resolverRef.current?.(false);
    resolverRef.current = null;
  }, []);

  const ask = (info: T) =>
    new Promise<boolean>((resolve) => {
      resolverRef.current?.(false);
      resolverRef.current = resolve;
      setPending(info);
    });

  const answer = (ok: boolean) => {
    const resolve = resolverRef.current;
    resolverRef.current = null;
    setPending(null);
    resolve?.(ok);
  };

  return { pending, ask, answer };
}
