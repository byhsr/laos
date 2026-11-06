import { create } from 'zustand';
import type { ChatStep } from '../types';
import { updateTaskResult, type StreamStep } from '../runtime';
import { mergeStep } from '../chatSteps';

// The live job stack a chat shows on its right: the current turn's activity, and
// every background task/tool. Items are per-chat (by agent id) and in-memory only
// — the Tasks view stays the durable list.
export type ActivityItem = {
  id: string;              // "turn-<agent>-<n>" or the task id
  agentId: string;         // the chat it belongs to
  // For a delegated task, the agent actually doing the work — so the job also
  // shows in that agent's panel, not only the chat that requested it.
  assignedAgent?: string;
  sessionId: string;
  kind: 'turn' | 'task';
  title: string;
  detail?: string;         // task input / the tool call
  status: 'running' | 'done' | 'failed';
  steps: ChatStep[];       // live events for a turn
  result?: string;
  startedAt: number;
  endedAt?: number;
  burst?: number;          // bumps once when it ends → drives the burst animation
};

const OPEN_KEY = 'laos.activity.open';
const WIDTH_KEY = 'laos.activityWidth';
export const MIN_WIDTH = 260;
export const MAX_WIDTH = 900;
const DEFAULT_WIDTH = 360;
const MAX_ITEMS = 20;

const readOpen = () => { try { return localStorage.getItem(OPEN_KEY) === '1'; } catch { return false; } };
const readWidth = () => {
  try {
    const v = Number(localStorage.getItem(WIDTH_KEY));
    return Number.isFinite(v) && v >= MIN_WIDTH ? Math.min(v, MAX_WIDTH) : DEFAULT_WIDTH;
  } catch { return DEFAULT_WIDTH; }
};
const write = (key: string, value: string) => { try { localStorage.setItem(key, value); } catch { /* ignore */ } };

// Set once the user closes the panel, so a background job doesn't keep re-opening
// something they dismissed.
let userClosed = false;
let turnSeq = 0;

type TaskStart = { taskId: string; chatAgentId: string; assignedAgent?: string; sessionId: string; kind: string; tool: string; label: string };
type TaskEnd = { taskId: string; status: string; result: string };

type ActivityState = {
  items: ActivityItem[];
  open: boolean;
  width: number;
  setOpen: (open: boolean) => void;
  toggle: () => void;
  openPanel: () => void;
  setWidth: (w: number) => void;
  startTurn: (agentId: string, sessionId: string, title: string) => string;
  stepTurn: (id: string, ev: StreamStep) => void;
  endTurn: (id: string, failed?: boolean) => void;
  startTask: (t: TaskStart) => void;
  finishTask: (t: TaskEnd) => void;
  reorder: (dragId: string, overId: string) => void;
  remove: (id: string) => void;
  // A job result too long for its card becomes a chip; this is the text view the
  // chip opens, where it can be edited and written back.
  editing: string | null;
  editText: string;
  beginEdit: (id: string) => void;
  setEditText: (text: string) => void;
  resetEdit: () => void;
  saveEdit: () => void;
  closeEdit: () => void;
};

export const useActivity = create<ActivityState>((set, get) => ({
  items: [],
  open: readOpen(),
  width: readWidth(),

  setOpen: (open) => {
    if (!open) userClosed = true;
    write(OPEN_KEY, open ? '1' : '0');
    set({ open });
  },
  toggle: () => get().setOpen(!get().open),
  // Auto-open for a new job — unless the user closed the panel already.
  openPanel: () => {
    if (userClosed || get().open) return;
    write(OPEN_KEY, '1');
    set({ open: true });
  },
  setWidth: (w) => {
    const width = Math.max(MIN_WIDTH, Math.min(Math.round(w), MAX_WIDTH));
    write(WIDTH_KEY, String(width));
    set({ width });
  },

  startTurn: (agentId, sessionId, title) => {
    const id = `turn-${agentId}-${++turnSeq}`;
    const item: ActivityItem = {
      id, agentId, sessionId, kind: 'turn', title, status: 'running', steps: [], startedAt: Date.now(),
    };
    set((s) => ({ items: [item, ...s.items].slice(0, MAX_ITEMS) }));
    return id;
  },
  stepTurn: (id, ev) => set((s) => ({
    items: s.items.map((i) => (i.id === id ? { ...i, steps: mergeStep(i.steps, ev) } : i)),
  })),
  endTurn: (id, failed) => set((s) => ({
    items: s.items.map((i) => (i.id === id
      ? { ...i, status: failed ? 'failed' : 'done', endedAt: Date.now(), burst: (i.burst ?? 0) + 1 }
      : i)),
  })),

  startTask: (t) => set((s) => {
    if (s.items.some((i) => i.id === t.taskId)) return {};
    const item: ActivityItem = {
      id: t.taskId, agentId: t.chatAgentId, assignedAgent: t.assignedAgent, sessionId: t.sessionId, kind: 'task',
      title: t.label || t.tool || 'background task',
      detail: t.tool ? t.tool : undefined,
      status: 'running', steps: [], startedAt: Date.now(),
    };
    return { items: [item, ...s.items].slice(0, MAX_ITEMS) };
  }),
  finishTask: (t) => set((s) => ({
    items: s.items.map((i) => (i.id === t.taskId
      ? { ...i, status: t.status === 'failed' ? 'failed' : 'done', result: t.result || i.result, endedAt: Date.now(), burst: (i.burst ?? 0) + 1 }
      : i)),
  })),

  reorder: (dragId, overId) => set((s) => {
    const from = s.items.findIndex((i) => i.id === dragId);
    const to = s.items.findIndex((i) => i.id === overId);
    if (from < 0 || to < 0 || from === to) return {};
    const items = [...s.items];
    const [moved] = items.splice(from, 1);
    items.splice(to, 0, moved);
    return { items };
  }),
  remove: (id) => set((s) => ({
    items: s.items.filter((i) => i.id !== id),
    ...(s.editing === id ? { editing: null, editText: '' } : {}),
  })),

  editing: null,
  editText: '',
  beginEdit: (id) => {
    const item = get().items.find((i) => i.id === id);
    if (!item) return;
    if (!get().open) { write(OPEN_KEY, '1'); set({ open: true }); }
    set({ editing: id, editText: item.result ?? '' });
  },
  setEditText: (text) => set({ editText: text }),
  resetEdit: () => set((s) => ({ editText: s.items.find((i) => i.id === s.editing)?.result ?? '' })),
  saveEdit: () => {
    const { editing, editText } = get();
    const item = get().items.find((i) => i.id === editing);
    if (!item) { set({ editing: null, editText: '' }); return; }
    // Tasks are the durable ones — write the edit back to the row.
    if (item.kind === 'task') void updateTaskResult(item.id, editText);
    set((s) => ({
      items: s.items.map((i) => (i.id === item.id ? { ...i, result: editText } : i)),
      editing: null,
      editText: '',
    }));
  },
  closeEdit: () => set({ editing: null, editText: '' }),
}));
