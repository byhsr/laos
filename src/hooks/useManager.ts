import { create } from 'zustand';
import { cancelChatStream, clearAgentMemory, closeSession, createChatSession, newChatStreamId, renameChatSession, streamChat } from '../runtime';
import { useConfirmStore } from './useConfirm';
import { useProjectsStore } from './useProjects';
import { useRunsStore } from './useRuns';
import type { Agent, ChatStep } from '../types';
import { createDeltaBuffer } from '../streamBuffer';
import { mergeStep } from '../chatSteps';

export type ChatEntry = { role: 'user' | 'assistant'; content: string; time: string };

// Derives a short, readable title from the first user message.
const titleFrom = (msg: string) => {
  const clean = msg.replace(/\s+/g, ' ').trim();
  return clean.length > 40 ? `${clean.slice(0, 40)}…` : (clean || 'Chat');
};

// Set while a streamed turn runs so the delta batcher stops painting once the
// turn is cancelled (the message it targets may already be gone). The token
// guards a turn's late callbacks from touching a newer turn's state.
let cancelledTurn = false;
let turnSeq = 0;

type ManagerState = {
  currentAgentId: string | null;
  conversations: Record<string, ChatEntry[]>; // per-agent history, never destroyed on switch
  messages: ChatEntry[]; // current view (active session)
  sessionIds: Record<string, string | null>; // per-agent active session
  busy: boolean;
  steps: ChatStep[]; // live trace of the current turn ("Thinking…", "Calling x…", its reasoning)
  activeStreamId: string | null; // id of the in-flight turn, for cancellation
  activeAgentId: string | null; // whose conversation the in-flight turn writes to
  setCurrentAgent: (agentId: string | null) => void;
  newSession: (agentId: string, model: string) => Promise<void>;
  loadHistory: (agentId: string) => Promise<void>;
  resumeSession: (agentId: string, sessionId: string, entries: ChatEntry[]) => void;
  reset: (agentId: string) => Promise<void>;
  send: (message: string, managerAgent: Agent) => Promise<void>;
  cancel: (action: 'pause' | 'delete') => void;
};

export const useManagerStore = create<ManagerState>((set, get) => ({
  currentAgentId: null,
  conversations: {},
  messages: [],
  sessionIds: {},
  busy: false,
  steps: [],
  activeStreamId: null,
  activeAgentId: null,

  setCurrentAgent: (agentId) => {
    const conv = get().conversations;
    set({ currentAgentId: agentId, conversations: conv, messages: agentId ? (conv[agentId] ?? []) : [] });
  },

  loadHistory: async (agentId) => {
    // A fresh start: never adopt a previous session as the live chat. Old chats
    // stay in History and are only continued when the user explicitly asks for
    // one. If we already hold an in-memory conversation for this agent (e.g.
    // switching back to it mid-session), keep it rather than wiping it.
    set((s) => {
      const entries = s.conversations[agentId] ?? [];
      const sessionId = s.sessionIds[agentId] ?? null;
      return {
        conversations: { ...s.conversations, [agentId]: entries },
        sessionIds: { ...s.sessionIds, [agentId]: sessionId },
        messages: (s.currentAgentId === agentId || agentId === 'manager') ? entries : s.messages,
      };
    });
  },

  // Continue a past chat on demand: make it the active session and load its
  // messages into the view, so the next message appends to it. Nothing is
  // adopted unless the user asks.
  resumeSession: (agentId, sessionId, entries) => {
    set((s) => {
      const conv = { ...s.conversations, [agentId]: entries };
      return {
        conversations: conv,
        sessionIds: { ...s.sessionIds, [agentId]: sessionId },
        messages: (s.currentAgentId === agentId || agentId === 'manager') ? entries : s.messages,
      };
    });
  },

  reset: async (agentId) => {
    await clearAgentMemory(agentId);
    set((s) => {
      const conv = { ...s.conversations, [agentId]: [] };
      return { conversations: conv, messages: s.currentAgentId === agentId ? [] : s.messages, sessionIds: { ...s.sessionIds, [agentId]: null } };
    });
  },

  newSession: async (agentId, model) => {
    // Fire-and-forget summarize the old session (don't block the UI — New chat
    // must feel instant). Create the new session and clear the view immediately.
    const cur = get().sessionIds[agentId];
    if (cur) { void closeSession(cur, agentId, model); }
    const sess = await createChatSession(agentId, 'Chat', useProjectsStore.getState().activeId || null);
    set((s) => {
      const conv = { ...s.conversations, [agentId]: [] };
      return { conversations: conv, sessionIds: { ...s.sessionIds, [agentId]: sess.id }, messages: [] };
    });
  },

  send: async (message, managerAgent) => {
    // Lazily create a session on the first message so every chat is recorded,
    // titled by the first user message.
    if (!get().sessionIds[managerAgent.id]) {
      const sess = await createChatSession(managerAgent.id, titleFrom(message), useProjectsStore.getState().activeId || null);
      set((s) => ({ sessionIds: { ...s.sessionIds, [managerAgent.id]: sess.id } }));
    } else {
      // If this is the first real message in a default-titled session, name it.
      const conv = get().conversations[managerAgent.id] ?? [];
      if (conv.filter((m) => m.role === 'user').length === 0) {
        await renameChatSession(get().sessionIds[managerAgent.id]!, titleFrom(message));
      }
    }
    const agentId = managerAgent.id;
    const myTurn = ++turnSeq;
    cancelledTurn = false;
    const streamId = newChatStreamId();
    set({ busy: true, steps: [], activeStreamId: streamId, activeAgentId: agentId });
    const userEntry: ChatEntry = { role: 'user', content: message, time: new Date().toLocaleTimeString() };
    // Seed an empty assistant bubble that grows as tokens stream in.
    const assistantEntry: ChatEntry = { role: 'assistant', content: '', time: new Date().toLocaleTimeString() };
    set((s) => {
      const conv = { ...s.conversations, [agentId]: [...(s.conversations[agentId] ?? []), userEntry, assistantEntry] };
      return { conversations: conv, messages: conv[agentId] };
    });
    // Batch the streamed deltas so we aren't re-rendering the whole conversation
    // (and re-parsing its markdown) once per token.
    const batcher = createDeltaBuffer((text) => {
      if (cancelledTurn || myTurn !== turnSeq) return;
      set((s) => {
        const list = s.conversations[agentId] ?? [];
        const updated = list.map((e, i) => (i === list.length - 1 ? { ...e, content: text } : e));
        return { conversations: { ...s.conversations, [agentId]: updated }, messages: updated };
      });
    });
    let failed = false;
    try {
      await streamChat(managerAgent, message, true, (delta) => batcher.push(delta), (confirmReq) => {
        // Pop the confirmation dialog; the backend waits for the decision.
        useConfirmStore.getState().request(confirmReq);
      }, get().sessionIds[managerAgent.id] ?? undefined, (ev) => set((s) => ({ steps: mergeStep(s.steps, ev) })), streamId);
      useRunsStore.getState().loadRuns();
    } catch (e) {
      failed = true;
      const msg = typeof e === 'string' ? e : (e instanceof Error ? e.message : 'Manager failed to respond.');
      set((s) => {
        const list = s.conversations[agentId] ?? [];
        const updated = list.map((e, i) => (i === list.length - 1 ? { ...e, content: `⚠️ ${msg}` } : e));
        return { conversations: { ...s.conversations, [agentId]: updated }, messages: updated };
      });
    } finally {
      // A newer turn owns the state now — its own finally will clean up.
      if (myTurn !== turnSeq) return;
      // Only flush the tail on success: after a failure the bubble already shows
      // the error, and a late flush would overwrite it with partial text.
      if (!failed) batcher.end();
      set({ busy: false, steps: [], activeStreamId: null, activeAgentId: null });
    }
  },

  // Pause keeps what has streamed so far (marked as stopped); delete drops the
  // user turn and its reply entirely. Both halt the backend turn first.
  cancel: (action) => {
    const { activeStreamId, activeAgentId } = get();
    if (!activeStreamId || !activeAgentId || cancelledTurn) return;
    cancelledTurn = true;
    void cancelChatStream(activeStreamId);
    // Dismiss any pending tool-approval popup so the backend isn't left waiting.
    useConfirmStore.setState({ pending: null });
    set((s) => {
      const list = s.conversations[activeAgentId] ?? [];
      const updated = action === 'delete'
        ? list.slice(0, -2)
        : list.map((e, i) => (i === list.length - 1 && e.role === 'assistant'
            ? { ...e, content: e.content ? `${e.content}\n\n_(stopped)_` : '_(stopped)_' }
            : e));
      return { conversations: { ...s.conversations, [activeAgentId]: updated }, messages: updated };
    });
    set({ busy: false, steps: [], activeStreamId: null, activeAgentId: null });
  },
}));
