import { create } from 'zustand';

// Per-agent chat backdrop images, keyed by agent id. Purely presentational, so
// like the other UI prefs (pen, nav labels, glass) they live in localStorage
// rather than on the agent record. Images are downscaled by the caller's
// readBackgroundImage before they get here, so the quota stays comfortable.
const KEY = 'laos.agentBackdrops';

const read = (): Record<string, string> => {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(KEY) || '{}');
    return raw && typeof raw === 'object' ? (raw as Record<string, string>) : {};
  } catch { return {}; }
};

const write = (m: Record<string, string>) => {
  try { localStorage.setItem(KEY, JSON.stringify(m)); } catch { /* ignore */ }
};

type State = {
  backdrops: Record<string, string>;
  setBackdrop: (agentId: string, image: string) => void;
};

export const useAgentBackdrop = create<State>((set, get) => ({
  backdrops: read(),
  setBackdrop: (agentId, image) => {
    const next = { ...get().backdrops };
    if (image) next[agentId] = image; else delete next[agentId];
    write(next);
    set({ backdrops: next });
  },
}));

/** The backdrop for one agent, or '' when it falls back to the theme surface. */
export const useAgentChatBackdrop = (agentId: string): string =>
  useAgentBackdrop((s) => s.backdrops[agentId] ?? '');
