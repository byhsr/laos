import { create } from 'zustand';
import { managerMessage as managerMessageApi } from '../runtime';

export type ChatEntry = { role: 'user' | 'assistant'; content: string; time: string };

type ManagerState = {
  currentAgentId: string | null;
  conversations: Record<string, ChatEntry[]>; // per-agent history, never destroyed on switch
  messages: ChatEntry[]; // current view
  busy: boolean;
  setCurrentAgent: (agentId: string | null) => void;
  send: (message: string) => Promise<string>;
};

export const useManagerStore = create<ManagerState>((set, get) => ({
  currentAgentId: null,
  conversations: {},
  messages: [],
  busy: false,

  setCurrentAgent: (agentId) => {
    const prev = get().currentAgentId;
    const conv = get().conversations;
    // Preserve the previous conversation, load the new one (empty if first visit).
    set({ currentAgentId: agentId, conversations: conv, messages: agentId ? (conv[agentId] ?? []) : [] });
    void prev;
  },

  send: async (message) => {
    set({ busy: true });
    const userEntry: ChatEntry = { role: 'user', content: message, time: new Date().toLocaleTimeString() };
    const agentId = get().currentAgentId ?? 'manager';
    set((s) => {
      const conv = { ...s.conversations, [agentId]: [...(s.conversations[agentId] ?? []), userEntry] };
      return { conversations: conv, messages: conv[agentId] };
    });
    let reply = '';
    try {
      reply = await managerMessageApi(message);
    } catch (e) {
      reply = e instanceof Error ? e.message : 'Manager failed to respond.';
    }
    const assistantEntry: ChatEntry = { role: 'assistant', content: reply, time: new Date().toLocaleTimeString() };
    set((s) => {
      const conv = { ...s.conversations, [agentId]: [...(s.conversations[agentId] ?? []), assistantEntry] };
      return { conversations: conv, messages: conv[agentId], busy: false };
    });
    return reply;
  },
}));
