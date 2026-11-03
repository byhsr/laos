// One utterance of hands-free voice: wait for speech, record it, stop on silence.
//
// Uses the voice session's existing stream + analyser, so the microphone is
// opened once per session rather than once per sentence. The caller owns teardown.

const TICK_MS = 50;
// Normalised RMS (0..1). Speech sits well above this; room noise sits below.
const SPEECH_RMS = 0.02;
// How long a pause ends the utterance. Long enough not to clip a sentence.
const SILENCE_MS = 1200;
// If nobody speaks for this long, resolve null and let the loop listen again.
const MAX_WAIT_MS = 15000;
// Hard cap so a stuck-open mic cannot record forever.
const MAX_UTTERANCE_MS = 45000;

function pickMime(): string {
  if (typeof MediaRecorder === 'undefined') return '';
  const candidates = ['audio/webm;codecs=opus', 'audio/webm', 'audio/ogg'];
  return candidates.find((m) => MediaRecorder.isTypeSupported(m)) ?? '';
}

function rms(analyser: AnalyserNode, buf: Uint8Array): number {
  analyser.getByteTimeDomainData(buf);
  let sum = 0;
  for (let i = 0; i < buf.length; i++) {
    const v = (buf[i] - 128) / 128;
    sum += v * v;
  }
  return Math.sqrt(sum / buf.length);
}

export function recordUtterance(
  stream: MediaStream,
  analyser: AnalyserNode,
  opts: { onLevel?: (level: number) => void; shouldStop: () => boolean },
): Promise<Blob | null> {
  const buf = new Uint8Array(analyser.fftSize);
  const mimeType = pickMime();

  return new Promise((resolve) => {
    let recorder: MediaRecorder | null = null;
    const chunks: Blob[] = [];
    let speechSeen = false;
    let lastVoice = 0;
    let startedAt = 0;
    let done = false;

    const finish = (blob: Blob | null) => {
      if (done) return;
      done = true;
      window.clearInterval(timer);
      if (recorder && recorder.state !== 'inactive') {
        try { recorder.stop(); } catch { /* already stopping */ }
      }
      opts.onLevel?.(0);
      resolve(blob);
    };

    const startRecorder = () => {
      if (recorder) return;
      recorder = mimeType ? new MediaRecorder(stream, { mimeType }) : new MediaRecorder(stream);
      recorder.ondataavailable = (e) => { if (e.data.size > 0) chunks.push(e.data); };
      recorder.onstop = () => {
        const type = recorder?.mimeType || 'audio/webm';
        const blob = new Blob(chunks, { type });
        finish(blob.size > 0 ? blob : null);
      };
      recorder.start();
      startedAt = Date.now();
      lastVoice = Date.now();
    };

    const stopRecorder = () => {
      if (recorder && recorder.state !== 'inactive') {
        try { recorder.stop(); } catch { /* already stopping */ }
      } else {
        finish(null);
      }
    };

    const timerStarted = Date.now();
    const timer = window.setInterval(() => {
      if (opts.shouldStop()) { stopRecorder(); return; }

      const level = Math.min(1, rms(analyser, buf) * 8);
      opts.onLevel?.(level);
      const now = Date.now();

      if (!speechSeen) {
        if (level >= SPEECH_RMS) { speechSeen = true; startRecorder(); }
        else if (now - timerStarted >= MAX_WAIT_MS) finish(null);
        return;
      }

      if (level >= SPEECH_RMS) lastVoice = now;
      if (now - startedAt >= MAX_UTTERANCE_MS) { stopRecorder(); return; }
      if (now - lastVoice >= SILENCE_MS) stopRecorder();
    }, TICK_MS);
  });
}
