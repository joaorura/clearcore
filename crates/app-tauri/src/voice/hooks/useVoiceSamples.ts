import { useCallback, useState } from 'react';
import type { SampleList, ServiceSample } from '../enrollmentTypes';
import { enrollmentErrorCode } from '../enrollmentErrors';
import { deleteSample, listSamples } from '../enrollmentClient';

export interface VoiceSamples {
  sampleList: SampleList | null;
  samples: ServiceSample[];
  samplesLoadFailed: boolean;
  deletingId: string | null;
  refreshSamples: () => Promise<SampleList | null>;
  /**
   * Deletes a sample in the service and refreshes the list (and budget). It does NOT rebuild or
   * re-send the profile: only the user frees budget, and rebuilding is the explicit button (spec §4.4).
   */
  removeSample: (id: string) => Promise<SampleList | null>;
}

/** Samples and budget come from the service only. */
export function useVoiceSamples(): VoiceSamples {
  const [sampleList, setSampleList] = useState<SampleList | null>(null);
  const [samplesLoadFailed, setSamplesLoadFailed] = useState<boolean>(false);
  const [deletingId, setDeletingId] = useState<string | null>(null);

  const refreshSamples = useCallback(async (): Promise<SampleList | null> => {
    try {
      const res = await listSamples();
      if (enrollmentErrorCode(res) !== null || !('samples' in res)) {
        setSamplesLoadFailed(true);
        return null;
      }
      setSampleList(res);
      setSamplesLoadFailed(false);
      return res;
    } catch {
      setSamplesLoadFailed(true);
      return null;
    }
  }, []);

  const removeSample = async (id: string): Promise<SampleList | null> => {
    setDeletingId(id);
    try {
      await deleteSample(id);
    } catch {
      // the refreshed list shows what the service still has
    }
    const list = await refreshSamples();
    setDeletingId(null);
    return list;
  };

  return { sampleList, samples: sampleList?.samples ?? [], samplesLoadFailed, deletingId, refreshSamples, removeSample };
}
