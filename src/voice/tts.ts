import { voiceSpeak } from '../runtime';

// Speech output for voice mode. Provider TTS when a tts model is configured and
// allowed, otherwise the webview's system voices.
export type TtsMode = 'auto' | 'system' | 'provider';

const VOICE_KEY = 'laos.voice.tts.voice';
const RATE_KEY = 'laos.voice.tts.rate';

export function ttsPrefs(): { voice: string; rate: number } {
  const stored = Number(localStorage.getItem(RATE_KEY));
  return {
    voice: localStorage.getItem(VOICE_KEY) ?? '',
    rate: Number.isFinite(stored) && stored > 0 ? stored : 1,
  };
}

export function saveTtsPrefs(patch: Partial<{ voice: string; rate: number }>) {
  if (patch.voice !== undefined) localStorage.setItem(VOICE_KEY, patch.voice);
  if (patch.rate !== undefined) localStorage.setItem(RATE_KEY, String(patch.rate));
}

// The text a voice should say: code blocks dropped, markdown stripped, capped so
// a long answer does not turn into a monologue.
export function stripForSpeech(text: string): string {
  return text
    .replace(/```[\s\S]*?```/g, ' (code omitted) ')
    .replace(/`([^`]+)`/g, '$1')
    .replace(/!\[[^\]]*\]\([^)]*\)/g, '')
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/<\/?[^>]+>/g, ' ')
    .replace(/[*_~#>|]+/g, ' ')
    .replace(/^\s*[-+]\s+/gm, '')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, 600);
}

let current: HTMLAudioElement | null = null;

function playBase64(data: string): Promise<void> {
  return new Promise((resolve) => {
    const audio = new Audio(`data:audio/mpeg;base64,${data}`);
    current = audio;
    const done = () => {
      audio.onended = null;
      audio.onerror = null;
      if (current === audio) current = null;
      resolve();
    };
    audio.onended = done;
    audio.onerror = done;
    void audio.play().catch(done);
  });
}

function speakSystem(text: string): Promise<void> {
  return new Promise((resolve) => {
    if (typeof speechSynthesis === 'undefined') { resolve(); return; }
    const utter = new SpeechSynthesisUtterance(text);
    const { voice, rate } = ttsPrefs();
    const match = voice ? speechSynthesis.getVoices().find((v) => v.name === voice) : undefined;
    if (match) utter.voice = match;
    utter.rate = rate;
    utter.onend = () => resolve();
    utter.onerror = () => resolve();
    speechSynthesis.speak(utter);
  });
}

export async function speakReply(text: string, mode: TtsMode): Promise<void> {
  const clean = stripForSpeech(text);
  if (!clean) return;
  if (mode !== 'system') {
    try {
      const audio = await voiceSpeak(clean);
      if (audio) { await playBase64(audio); return; }
    } catch {
      // No provider TTS (or it failed) — the system voice still works.
    }
  }
  await speakSystem(clean);
}

export function stopSpeech(): void {
  try { speechSynthesis?.cancel(); } catch { /* no system voices */ }
  if (current) {
    try { current.pause(); } catch { /* already gone */ }
    current = null;
  }
}
