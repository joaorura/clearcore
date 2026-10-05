import type { VoiceProfileStatus } from '../types';
import type { EnrollmentLabels } from './enrollmentTypes';

/**
 * Development notice. Besides the enrollment model, it says when the ISOLATION model is the
 * development pDFNet3 (GetStatus `dev_base_model === 'pdfnet3-dev'`): the M2 NO-GO checkpoint,
 * loaded only so a voice profile can be applied; never an approved or production model. When the
 * configured development model could not be used, the fixed service code is shown.
 */
export function DevModelNotice({
  labels,
  devBaseModel,
  devBaseModelError,
}: {
  labels: EnrollmentLabels;
  devBaseModel?: VoiceProfileStatus['dev_base_model'];
  devBaseModelError?: string | null;
}) {
  return (
    <div role="note" className="dev-model-notice" style={{ fontSize: 12, color: '#fbbf24' }}>
      <div>{labels.devModelNotice}</div>
      {devBaseModel === 'pdfnet3-dev' && <div>{labels.devIsolationModelNotice}</div>}
      {devBaseModel !== 'pdfnet3-dev' && devBaseModelError && (
        <div>{labels.devIsolationModelError.replace('{code}', devBaseModelError)}</div>
      )}
    </div>
  );
}
