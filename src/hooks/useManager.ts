import { create } from 'zustand';
import { loadConversation, streamChat } from '../runtime';
import { useRunsStore } from './useRuns';
import type { Agent } from '../types';

export type ChatEntry = { role: 'user' | 'assistant'; content: string; time: string };

type ManagerState = {
  currentAgentId: string | null;
  conversations: Record<string, ChatEntry[]>; // per-agent history, never destroyed on switch
  messages: ChatEntry[]; // current view
  busy: boolean;
  setCurrentAgent: (agentId: string | null) => void;
  loadHistory: (agentId: string) => Promise<void>;
  send: (message: string, managerAgent: Agent) => Promise<void>;
};

export const useManagerStore = create<ManagerState>((set, get) => ({
  currentAgentId: null,
  conversations: {},
  messages: [],
  busy: false,

  setCurrentAgent: (agentId) => {
    const conv = get().conversations;
    set({ currentAgentId: agentId, conversations: conv, messages: agentId ? (conv[agentId] ?? []) : [] });
  },

  loadHistory: async (agentId) => {
    const stored = await loadConversation(agentId);
    if (stored.length === 0) return;
    const entries: ChatEntry[] = stored.map((m) => ({ role: m.role, content: m.content, time: '' }));
    set((s) => {
      const conv = { ...s.conversations, [agentId]: entries };
      return { conversations: conv, messages: s.currentAgentId === agentId ? entries : s.messages };
    });
  },

  send: async (message, managerAgent) => {
    set({ busy: true });
    const userEntry: ChatEntry = { role: 'user', content: message, time: new Date().toLocaleTimeString() };
    const agentId = managerAgent.id;
    // Seed an empty assistant bubble that grows as tokens stream in.
    const assistantEntry: ChatEntry = { role: 'assistant', content: '', time: new Date().toLocaleTimeString() };
    set((s) => {
      const conv = { ...s.conversations, [agentId]: [...(s.conversations[agentId] ?? []), userEntry, assistantEntry] };
      return { conversations: conv, messages: conv[agentId] };
    });
    let buffer = '';
    try {
      await streamChat(managerAgent, message, true, (delta) => {
        buffer += delta;
        set((s) => {
          const list = s.conversations[agentId] ?? [];
          const updated = list.map((e, i) => (i === list.length - 1 ? { ...e, content: buffer } : e));
          return { conversations: { ...s.conversations, [agentId]: updated }, messages: updated };
        });
      });
      useRunsStore.getState().loadRuns();
    } catch (e) {
      const msg = typeof e === 'string' ? e : (e instanceof Error ? e.message : 'Manager failed to respond.');
      set((s) => {
        const list = s.conversations[agentId] ?? [];
        const updated = list.map((e, i) => (i === list.length - 1 ? { ...e, content: `⚠️ ${msg}` } : e));
        return { conversations: { ...s.conversations, [agentId]: updated }, messages: updated };
      });
    } finally {
      set({ busy: false });
    }
  },
}));
