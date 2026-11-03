import { resetMicrophonePermission } from '../runtime';

// Asks for the microphone. The webview remembers a denied permission per origin,
// so a refusal is cleared once (restoring the prompt) and retried; a second
// refusal is reported instead of leaving the feature dead.
export async function openMicStream(): Promise<{ stream: MediaStream } | { error: string }> {
  const constraints: MediaStreamConstraints = {
    audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true },
  };
  try {
    return { stream: await navigator.mediaDevices.getUserMedia(constraints) };
  } catch (e) {
    if ((e as DOMException)?.name !== 'NotAllowedError') {
      return { error: 'microphone unavailable' };
    }
    await resetMicrophonePermission();
    try {
      return { stream: await navigator.mediaDevices.getUserMedia(constraints) };
    } catch {
      return { error: 'microphone blocked — allow it in Windows Settings → Privacy → Microphone' };
    }
  }
}
