import type { VoiceProfileStatus } from '../../types';
import { voiceProfileStatusLabelKey, type Translate } from '../hooks/voiceProfileLogic';

const LABEL_PATH = {
  active: 'voiceProfile.statusActive',
  storedNotApplied: 'voiceProfile.storedNotApplied',
  enrolledUnconfirmed: 'voiceProfile.enrolledUnconfirmed',
  none: 'voiceProfile.statusPending',
} as const;

/** Only a status the service confirmed as active is shown as active. */
export function profileStatusLabel(status: VoiceProfileStatus, t: Translate): { text: string; active: boolean } {
  const key = voiceProfileStatusLabelKey(status);
  return { text: t(LABEL_PATH[key]), active: key === 'active' };
}
