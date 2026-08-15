import { create } from 'zustand';
import type { Tool } from '../types';
import { listTools, saveTool as saveToolApi, deleteTool as deleteToolApi } from '../runtime';
import { initialTools } from '../data';

type ToolsState = {
  tools: Tool[];
  setTools: (tools: Tool[] | ((prev: Tool[]) => Tool[])) => void;
  loadTools: () => Promise<void>;
  saveTool: (t: Tool) => Promise<void>;
  deleteTool: (id: string) => Promise<void>;
};

export const useToolsStore = create<ToolsState>((set) => ({
  tools: initialTools,

  setTools: (tools) => set((s) => ({ tools: typeof tools === 'function' ? tools(s.tools) : tools })),

  loadTools: async () => {
    const stored = await listTools().catch(() => [] as Tool[]);
    if (stored.length) set({ tools: stored });
  },

  saveTool: async (t) => {
    await saveToolApi(t);
    set((s) => ({ tools: s.tools.some((p) => p.id === t.id) ? s.tools.map((p) => (p.id === t.id ? t : p)) : [...s.tools, t] }));
  },

  deleteTool: async (id) => {
    await deleteToolApi(id);
    set((s) => ({ tools: s.tools.filter((p) => p.id !== id) }));
  },
}));
