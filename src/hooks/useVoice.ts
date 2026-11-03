import { create } from 'zustand';
import type { Agent } from '../types';
import { voiceTranscribe } from '../runtime';
import { openMicStream } from '../voice/microphone';
import { recordUtterance } from '../voice/vad';
import { speakReply, stopSpeech, type TtsMode } from '../voice/tts';
import { useActiveChat } from './useActiveChat';
import { useAgentsStore } from './useAgents';
import { useManagerStore } from './useManager';

// Live, hands-free voice: listen → transcribe → send → speak the reply → listen
// again, until toggled off. One session at a time.

export type VoicePhase = 'idle' | 'listening' | 'transcribing' | 'thinking' | 'speaking' | 'error';

type Session = {
  stopped: boolean;
  stream: MediaStream | null;
  ctx: AudioContext | null;
  analyser: AnalyserNode | null;
};

let session: Session | null = null;

type VoiceState = {
  active: boolean;
  phase: VoicePhase;
  level: number;
  transcript: string;
  error?: string;
  agentId: string | null;
  ttsMode: TtsMode;
  toggle: () => void;
  start: (agentId?: string) => Promise<void>;
  stop: () => void;
  setTtsMode: (mode: TtsMode) => void;
};

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const url = String(reader.result ?? '');
      const comma = url.indexOf(',');
      if (comma < 0) { reject(new Error('could not read the recording')); return; }
      resolve(url.slice(comma + 1));
    };
    reader.onerror = () => reject(new Error('could not read the recording'));
    reader.readAsDataURL(blob);
  });
}

// The chat to talk to: the one in front of the user, else the lead agent.
function resolveAgent(explicit?: string): Agent | null {
  const agents = useAgentsStore.getState().agents;
  const id = explicit ?? useActiveChat.getState().agentId ?? undefined;
  return agents.find((a) => a.id === id) ?? agents.find((a) => a.isManager) ?? null;
}

const turnBusy = (agentId: string) => {
  const s = useManagerStore.getState();
  return s.busy && s.activeAgentId === agentId;
};

export const useVoice = create<VoiceState>((set, get) => {
  const teardown = (s: Session | null) => {
    if (!s) return;
    s.stopped = true;
    try { s.stream?.getTracks().forEach((t) => t.stop()); } catch { /* already gone */ }
    try { void s.ctx?.close(); } catch { /* already closed */ }
  };

  const loop = async (agent: Agent, sess: Session) => {
    while (!sess.stopped) {
      // Never listen over an in-flight turn (voice or typed).
      while (!sess.stopped && turnBusy(agent.id)) await sleep(150);
      if (sess.stopped) break;

      set({ phase: 'listening', transcript: '', level: 0 });
      const blob = await recordUtterance(sess.stream!, sess.analyser!, {
        onLevel: (level) => set({ level }),
        shouldStop: () => sess.stopped,
      });
      if (sess.stopped) break;
      if (!blob) continue;

      set({ phase: 'transcribing', level: 0 });
      let text = '';
      try {
        const data = await blobToBase64(blob);
        const mime = (blob.type || 'audio/webm').split(';')[0];
        text = (await voiceTranscribe('voice.webm', mime, data)).trim();
      } catch (e) {
        if (sess.stopped) break;
        // Tear down but stay visible in the error state — the normal post-loop
        // stop() would reset to idle and hide the message.
        teardown(sess);
        if (session === sess) session = null;
        stopSpeech();
        set({ active: true, phase: 'error', error: e instanceof Error ? e.message : String(e), level: 0 });
        return;
      }
      if (!text) continue;

      set({ transcript: text, phase: 'thinking' });
      try {
        await useManagerStore.getState().send(text, agent, [], agent.isManager ?? false);
      } catch { /* the chat surfaces its own failure in the bubble */ }
      if (sess.stopped) break;

      const conversation = useManagerStore.getState().conversations[agent.id] ?? [];
      const reply = [...conversation].reverse().find((m) => m.role === 'assistant')?.content ?? '';
      if (!reply.trim()) continue;

      set({ phase: 'speaking' });
      await speakReply(reply, get().ttsMode);
      if (sess.stopped) break;
    }
    // Reached only when the loop ended on its own (e.g. a fatal error).
    if (!sess.stopped) get().stop();
  };

  return {
    active: false,
    phase: 'idle',
    level: 0,
    transcript: '',
    agentId: null,
    ttsMode: 'auto',

    setTtsMode: (mode) => set({ ttsMode: mode }),

    toggle: () => {
      if (get().active) get().stop();
      else void get().start();
    },

    start: async (agentId) => {
      if (get().active) return;
      // Clean up any half-open session before starting a new one.
      teardown(session);
      session = null;

      const agent = resolveAgent(agentId);
      set({ active: true, phase: 'listening', level: 0, transcript: '', error: undefined, agentId: agent?.id ?? null });
      if (!agent) {
        set({ phase: 'error', error: 'No chat open. Open a chat first, or create an agent.' });
        return;
      }

      const mic = await openMicStream();
      // The user may have stopped while the permission prompt was open.
      if (!get().active) {
        if ('stream' in mic) mic.stream.getTracks().forEach((t) => t.stop());
        return;
      }
      if ('error' in mic) {
        set({ phase: 'error', error: mic.error });
        return;
      }

      let ctx: AudioContext;
      let analyser: AnalyserNode;
      try {
        ctx = new AudioContext();
        // A capture-only graph is normally allowed, but resume defensively: a
        // suspended context would leave the analyser reading silence.
        try { await ctx.resume(); } catch { /* stays suspended; the level meter is inert */ }
        const source = ctx.createMediaStreamSource(mic.stream);
        analyser = ctx.createAnalyser();
        analyser.fftSize = 1024;
        source.connect(analyser);
      } catch {
        mic.stream.getTracks().forEach((t) => t.stop());
        set({ phase: 'error', error: 'audio processing is unavailable in this webview' });
        return;
      }

      const sess: Session = { stopped: false, stream: mic.stream, ctx, analyser };
      session = sess;
      void loop(agent, sess);
    },

    stop: () => {
      const s = session;
      session = null;
      teardown(s);
      stopSpeech();
      set({ active: false, phase: 'idle', level: 0 });
    },
  };
});
