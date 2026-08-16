import { create } from 'zustand';
import { clearAgentMemory, closeSession, createChatSession, loadConversation, renameChatSession, streamChat } from '../runtime';
import { useConfirmStore } from './useConfirm';
import { useRunsStore } from './useRuns';
import type { Agent } from '../types';

export type ChatEntry = { role: 'user' | 'assistant'; content: string; time: string };

// Derives a short, readable title from the first user message.
const titleFrom = (msg: string) => {
  const clean = msg.replace(/\s+/g, ' ').trim();
  return clean.length > 40 ? `${clean.slice(0, 40)}…` : (clean || 'Chat');
};

type ManagerState = {
  currentAgentId: string | null;
  conversations: Record<string, ChatEntry[]>; // per-agent history, never destroyed on switch
  messages: ChatEntry[]; // current view
  sessionId: string | null;
  busy: boolean;
  setCurrentAgent: (agentId: string | null) => void;
  newSession: (agentId: string, model: string) => Promise<void>;
  loadHistory: (agentId: string) => Promise<void>;
  reset: (agentId: string) => Promise<void>;
  send: (message: string, managerAgent: Agent) => Promise<void>;
};

export const useManagerStore = create<ManagerState>((set, get) => ({
  currentAgentId: null,
  conversations: {},
  messages: [],
  sessionId: null,
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

  reset: async (agentId) => {
    await clearAgentMemory(agentId);
    set((s) => {
      const conv = { ...s.conversations, [agentId]: [] };
      return { conversations: conv, messages: s.currentAgentId === agentId ? [] : s.messages, sessionId: null };
    });
  },

  newSession: async (agentId, model) => {
    // Close the current session first: summarize it and fold into the day context.
    const cur = get().sessionId;
    if (cur) { await closeSession(cur, agentId, model); }
    const sess = await createChatSession(agentId, 'Chat');
    set({ sessionId: sess.id, messages: [] });
  },

  send: async (message, managerAgent) => {
    // Lazily create a session on the first message so every chat is recorded,
    // titled by the first user message.
    if (!get().sessionId) {
      const sess = await createChatSession(managerAgent.id, titleFrom(message));
      set({ sessionId: sess.id });
    } else {
      // If this is the first real message in a default-titled session, name it.
      const conv = get().conversations[managerAgent.id] ?? [];
      if (conv.filter((m) => m.role === 'user').length === 0) {
        await renameChatSession(get().sessionId!, titleFrom(message));
      }
    }
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
      }, (confirmReq) => {
        // Pop the confirmation dialog; the backend waits for the decision.
        useConfirmStore.getState().request(confirmReq);
      }, get().sessionId ?? undefined);
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
