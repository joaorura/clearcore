import { useCallback, useState } from 'react';
import { invokeBridge } from '../../bridge';
import type { CallSuggestionTake } from '../../types';
import { classifyTakeApproval, type TakeApprovalOutcome } from './voiceProfileLogic';

export interface CallTakes {
  callTakes: CallSuggestionTake[];
  refreshCallTakes: () => Promise<void>;
  /** Approves a take; the service rechecks the speech budget. The caller refreshes samples and takes. */
  approveTake: (take: CallSuggestionTake) => Promise<TakeApprovalOutcome>;
  dismissTake: (id: string) => Promise<void>;
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

  const approveTake = async (take: CallSuggestionTake): Promise<TakeApprovalOutcome> => {
    let res: unknown;
    try {
      res = await invokeBridge<unknown>('approve_call_take', { id: take.id, name: take.title });
    } catch {
      res = { errorCode: 'SERVICE_UNAVAILABLE' };
    }
    return classifyTakeApproval(res);
  };

  const dismissTake = async (id: string) => {
    await invokeBridge('dismiss_call_take', { id }).catch(() => undefined);
    await refreshCallTakes();
  };

  return { callTakes, refreshCallTakes, approveTake, dismissTake };
}
