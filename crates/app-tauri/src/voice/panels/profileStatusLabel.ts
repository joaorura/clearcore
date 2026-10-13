import type { VoiceProfileStatus } from '../../types';
import { voiceProfileStatusLabelKey, type Translate } from '../hooks/voiceProfileLogic';

const LABEL_PATH = {
  active: 'voiceProfile.statusActive',
  storedNotApplied: 'voiceProfile.storedNotApplied',
  none: 'voiceProfile.statusNone',
  corrupted: 'voiceProfile.statusCorrupted',
} as const;

/** Only a status the service confirmed as active is shown as active. */
export function profileStatusLabel(status: VoiceProfileStatus, t: Translate): { text: string; active: boolean; corrupted: boolean } {
  const key = voiceProfileStatusLabelKey(status);
  return { text: t(LABEL_PATH[key]), active: key === 'active', corrupted: key === 'corrupted' };
}
