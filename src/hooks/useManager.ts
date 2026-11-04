import { create } from 'zustand';
import { cancelChatStream, clearAgentMemory, closeSession, createChatSession, newChatStreamId, renameChatSession, streamChat } from '../runtime';
import { EMPTY_REPLY } from '../components/chat/MessageBubble';
import { useActivity } from './useActivity';
import { useConfirmStore } from './useConfirm';
import { useProjectsStore } from './useProjects';
import { useRunsStore } from './useRuns';
import type { Agent, Attachment, ChatStep } from '../types';
import { createDeltaBuffer } from '../streamBuffer';
import { mergeStep } from '../chatSteps';

export type ChatEntry = {
  role: 'user' | 'assistant';
  content: string;
  time: string;
  attachments?: Attachment[];
  // Wall-clock latency of the reply, captured live (not persisted). ttft = time
  // to the first streamed token.
  durationMs?: number;
  ttftMs?: number;
};

// Chats live in this in-memory store only. The chat panels stay mounted while
// you navigate, so a live conversation survives tab switches — but nothing is
// mirrored to localStorage and a fresh launch always starts empty. Past chats
// are resumed on demand from History (resumeSession), not restored automatically.

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

// Results that arrived while a turn was still running. A fast background task can
// finish before the turn that started it has stopped streaming, so instead of
// dropping the result we hold it and deliver it the moment the chat goes idle.
const pendingResults: { agent: Agent; sessionId: string; result: string }[] = [];

const flushResults = async () => {
  const store = useManagerStore.getState();
  if (store.busy || pendingResults.length === 0) return;
  const next = pendingResults.shift()!;
  await store.takeTaskResult(next.agent, next.sessionId, next.result);
};

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
  // `isManagerOverride` lets a global caller (live voice) run against a regular
  // agent; the Manager view leaves it unset and keeps the manager path.
  send: (message: string, managerAgent: Agent, attachments?: Attachment[], isManagerOverride?: boolean) => Promise<void>;
  // A finished background task: hand its result back to the agent and let it write
  // the follow-up itself, streaming into the conversation the task came from.
  takeTaskResult: (agent: Agent, sessionId: string, result: string) => Promise<void>;
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

  // Keep the live conversation when we already hold it (switching back to an
  // agent mid-session). A fresh launch starts empty: past sessions are not
  // restored automatically — History → continue chat resumes one on demand.
  loadHistory: async (agentId) => {
    set((s) => ({
      messages: (s.currentAgentId === agentId || agentId === 'manager') ? (s.conversations[agentId] ?? []) : s.messages,
    }));
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

  send: async (message, managerAgent, attachments, isManagerOverride) => {
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
    const t0 = performance.now();
    let ttftMs: number | undefined;
    set({ busy: true, steps: [], activeStreamId: streamId, activeAgentId: agentId });
    const turnItem = useActivity.getState().startTurn(agentId, get().sessionIds[managerAgent.id] ?? '', titleFrom(message));
    const userEntry: ChatEntry = { role: 'user', content: message, time: new Date().toLocaleTimeString(), attachments };
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
      if (ttftMs === undefined) ttftMs = performance.now() - t0;
      set((s) => {
        const list = s.conversations[agentId] ?? [];
        const updated = list.map((e, i) => (i === list.length - 1 ? { ...e, content: text } : e));
        return { conversations: { ...s.conversations, [agentId]: updated }, messages: updated };
      });
    });
    let failed = false;
    try {
      await streamChat(managerAgent, message, isManagerOverride ?? true, (delta) => batcher.push(delta), (confirmReq) => {
        // Pop the confirmation dialog; the backend waits for the decision.
        useConfirmStore.getState().request(confirmReq);
      }, get().sessionIds[managerAgent.id] ?? undefined, (ev) => { set((s) => ({ steps: mergeStep(s.steps, ev) })); useActivity.getState().stepTurn(turnItem, ev); }, streamId, attachments);
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
      // Stamp how long the reply took onto the assistant bubble. A turn that
      // produced no text must never render (or persist) blank.
      const durationMs = performance.now() - t0;
      set((s) => {
        const list = s.conversations[agentId] ?? [];
        const updated = list.map((e, i) => {
          if (i !== list.length - 1 || e.role !== 'assistant') return e;
          return { ...e, content: e.content.trim() ? e.content : EMPTY_REPLY, durationMs, ttftMs };
        });
        return { conversations: { ...s.conversations, [agentId]: updated }, messages: updated };
      });
      set({ busy: false, steps: [], activeStreamId: null, activeAgentId: null });
      useActivity.getState().endTurn(turnItem, failed);
      void flushResults();
    }
  },

  takeTaskResult: async (agent, sessionId, result) => {
    // If a turn is already streaming, hold the result and deliver it when the chat
    // frees up — a fast task must still be reported, not silently dropped.
    if (get().busy) { pendingResults.push({ agent, sessionId, result }); return; }
    const agentId = agent.id;
    if (sessionId && get().sessionIds[agentId] !== sessionId) {
      set((s) => ({ sessionIds: { ...s.sessionIds, [agentId]: sessionId } }));
    }
    const myTurn = ++turnSeq;
    cancelledTurn = false;
    const streamId = newChatStreamId();
    const t0 = performance.now();
    let ttftMs: number | undefined;
    set({ busy: true, steps: [], activeStreamId: streamId, activeAgentId: agentId });
    const turnItem = useActivity.getState().startTurn(agentId, sessionId, 'Replying to a finished task');
    // Only the reply is shown — the injected instruction never becomes a user turn
    // (the backend records it as a system turn).
    const assistantEntry: ChatEntry = { role: 'assistant', content: '', time: new Date().toLocaleTimeString() };
    set((s) => {
      const conv = { ...s.conversations, [agentId]: [...(s.conversations[agentId] ?? []), assistantEntry] };
      return { conversations: conv, messages: s.currentAgentId === agentId ? conv[agentId] : s.messages };
    });
    const batcher = createDeltaBuffer((text) => {
      if (cancelledTurn || myTurn !== turnSeq) return;
      if (ttftMs === undefined) ttftMs = performance.now() - t0;
      set((s) => {
        const list = s.conversations[agentId] ?? [];
        const updated = list.map((e, i) => (i === list.length - 1 ? { ...e, content: text } : e));
        return { conversations: { ...s.conversations, [agentId]: updated }, messages: s.currentAgentId === agentId ? updated : s.messages };
      });
    });
    const note = `A background task you started has finished.\n\n${result}\n\nReply to the user now, in plain text: say briefly what came back and take any obvious next step. Do not call tools in this turn.`;
    let failed = false;
    try {
      await streamChat(agent, note, !!agent.isManager, (d) => batcher.push(d), (req) => {
        useConfirmStore.getState().request(req);
      }, sessionId || undefined, (ev) => { set((s) => ({ steps: mergeStep(s.steps, ev) })); useActivity.getState().stepTurn(turnItem, ev); }, streamId, undefined, true);
      useRunsStore.getState().loadRuns();
    } catch (e) {
      failed = true;
      const msg = typeof e === 'string' ? e : (e instanceof Error ? e.message : 'the follow-up failed.');
      set((s) => {
        const list = s.conversations[agentId] ?? [];
        const updated = list.map((e, i) => (i === list.length - 1 ? { ...e, content: `⚠️ ${msg}` } : e));
        return { conversations: { ...s.conversations, [agentId]: updated }, messages: s.currentAgentId === agentId ? updated : s.messages };
      });
    } finally {
      if (myTurn !== turnSeq) return;
      if (!failed) batcher.end();
      const durationMs = performance.now() - t0;
      // The model can come back with nothing. Rather than an "(empty reply)" the
      // user still gets the actual data — which is the point of the follow-up.
      const body = result.trim();
      const fallback = body
        ? `Background task finished. Here's what came back:\n\n${body.slice(0, 1200)}${body.length > 1200 ? '\n\n…the full text is in the activity panel.' : ''}`
        : 'Background task finished, but it came back empty.';
      set((s) => {
        const list = s.conversations[agentId] ?? [];
        const updated = list.map((e, i) => {
          if (i !== list.length - 1 || e.role !== 'assistant') return e;
          return { ...e, content: e.content.trim() ? e.content : fallback, durationMs, ttftMs };
        });
        return { conversations: { ...s.conversations, [agentId]: updated }, messages: s.currentAgentId === agentId ? updated : s.messages };
      });
      set({ busy: false, steps: [], activeStreamId: null, activeAgentId: null });
      useActivity.getState().endTurn(turnItem, failed);
      void flushResults();
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
    void flushResults();
  },
}));
