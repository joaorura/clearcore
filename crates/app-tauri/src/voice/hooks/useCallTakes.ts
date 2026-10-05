import { useCallback, useRef, useState } from 'react';
import { invokeBridge } from '../../bridge';
import type { CallSuggestionTake } from '../../types';
import { classifyTakeApproval, type TakeApprovalOutcome } from './voiceProfileLogic';

export interface CallTakes {
  callTakes: CallSuggestionTake[];
  refreshCallTakes: () => Promise<void>;
  /** Approves a take; the service rechecks the speech budget. The caller refreshes samples and takes. */
  /** Resolves null when an approval of the same take is already in flight. */
  approveTake: (take: CallSuggestionTake) => Promise<TakeApprovalOutcome | null>;
  approvingIds: string[];
  dismissTake: (id: string) => Promise<void>;
}

/** One approval per take id at a time (a double click must not approve twice). */
export class ApprovalGuard {
  private readonly inFlight = new Set<string>();
  begin(id: string): boolean {
    if (this.inFlight.has(id)) return false;
    this.inFlight.add(id);
    return true;
  }
  end(id: string): void { this.inFlight.delete(id); }
  ids(): string[] { return [...this.inFlight]; }
}

/** Call takes come from the service only (empty list when none). */
export function useCallTakes(): CallTakes {
  const [callTakes, setCallTakes] = useState<CallSuggestionTake[]>([]);

  const refreshCallTakes = useCallback(async () => {
    try {
      const res = await invokeBridge<{ takes?: CallSuggestionTake[] } | CallSuggestionTake[]>('get_call_takes');
      if (Array.isArray(res)) setCallTakes(res);
      else if (res && typeof res === 'object' && Array.isArray(res.takes)) setCallTakes(res.takes);
      else setCallTakes([]);
    } catch {
      setCallTakes([]);
    }
  }, []);

  const guardRef = useRef(new ApprovalGuard());
  const [approvingIds, setApprovingIds] = useState<string[]>([]);

  const approveTake = async (take: CallSuggestionTake): Promise<TakeApprovalOutcome | null> => {
    const guard = guardRef.current;
    if (!guard.begin(take.id)) return null;
    setApprovingIds(guard.ids());
    let res: unknown;
    try {
      res = await invokeBridge<unknown>('approve_call_take', { id: take.id, name: take.title });
    } catch {
      res = { errorCode: 'SERVICE_UNAVAILABLE' };
    } finally {
      guard.end(take.id);
      setApprovingIds(guard.ids());
    }
    return classifyTakeApproval(res);
  };

  const dismissTake = async (id: string) => {
    await invokeBridge('dismiss_call_take', { id }).catch(() => undefined);
    await refreshCallTakes();
  };

  return { callTakes, refreshCallTakes, approveTake, approvingIds, dismissTake };
}
