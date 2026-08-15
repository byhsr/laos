import { create } from 'zustand';
import type { Agent } from '../types';
import { listAgents, saveAgent, deleteAgent as deleteAgentApi } from '../runtime';

type AgentsState = {
  agents: Agent[];
  setAgents: (agents: Agent[] | ((prev: Agent[]) => Agent[])) => void;
  loadAgents: () => Promise<void>;
  persistAgent: (agent: Agent) => Promise<void>;
  deleteAgent: (id: string) => Promise<void>;
  createAgent: (draft: Partial<Agent>) => Promise<Agent>;
};

export const useAgentsStore = create<AgentsState>((set) => ({
  agents: [],

  setAgents: (agents) => set((s) => ({ agents: typeof agents === 'function' ? agents(s.agents) : agents })),

  loadAgents: async () => {
    const stored = await listAgents().catch(() => [] as Agent[]);
    if (stored.length) set({ agents: stored });
  },

  persistAgent: async (agent) => {
    set((s) => ({ agents: s.agents.some((a) => a.id === agent.id) ? s.agents.map((a) => (a.id === agent.id ? agent : a)) : [...s.agents, agent] }));
    try { await saveAgent(agent); } catch { /* browser preview: no Tauri backend */ }
  },

  deleteAgent: async (id) => {
    set((s) => ({ agents: s.agents.filter((a) => a.id !== id) }));
    try { await deleteAgentApi(id); } catch { /* browser preview */ }
  },

  createAgent: async (draft) => {
    const id = `agent-${Date.now()}`;
    const agent: Agent = {
      id, name: draft.name ?? 'New Agent', objective: draft.objective ?? '', model: draft.model ?? 'ollama:qwen3:8b',
      toolIds: draft.toolIds ?? [], integrations: draft.integrations ?? [], memory: true,
      permissions: draft.permissions ?? ['network'], homePath: `agents/${id}`, color: draft.color ?? '#22c55e',
      x: 160 + Math.random() * 160, y: 120 + Math.random() * 120,
    };
    await useAgentsStore.getState().persistAgent(agent);
    return agent;
  },
}));
