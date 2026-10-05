/**
 * AudioWorklet processor source, loaded through a Blob URL (no separate file to bundle).
 * Posts a copy of the first channel of every 128-frame render quantum.
 */
export const PCM_WORKLET_SOURCE = `
class PcmCapture extends AudioWorkletProcessor {
  process(inputs) {
    const channel = inputs[0] && inputs[0][0];
    if (channel && channel.length > 0) {
      this.port.postMessage(new Float32Array(channel));
    }
    return true;
  }
}
registerProcessor('pcm-capture', PcmCapture);
`;
