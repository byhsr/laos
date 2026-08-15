import { useCallback, useEffect, useState } from 'react';
import type { Agent } from '../types';
import { listAgents, saveAgent, deleteAgent as deleteAgentApi } from '../runtime';

export function useAgents() {
  const [agents, setAgents] = useState<Agent[]>([]);

  useEffect(() => {
    listAgents().then((stored) => {
      if (stored.length) setAgents(stored);
    }).catch(() => {});
  }, []);

  const persistAgent = useCallback(async (agent: Agent) => {
    setAgents((prev) => prev.some((a) => a.id === agent.id) ? prev.map((a) => (a.id === agent.id ? agent : a)) : [...prev, agent]);
    try { await saveAgent(agent); } catch { /* browser preview: no Tauri backend */ }
  }, []);

  const deleteAgent = useCallback(async (id: string) => {
    setAgents((prev) => prev.filter((a) => a.id !== id));
    try { await deleteAgentApi(id); } catch { /* browser preview */ }
  }, []);

  const createAgent = useCallback(async (draft: Partial<Agent>) => {
    const id = `agent-${Date.now()}`;
    const agent: Agent = {
      id, name: draft.name ?? 'New Agent', objective: draft.objective ?? '', model: draft.model ?? 'ollama:qwen3:8b',
      toolIds: draft.toolIds ?? [], integrations: draft.integrations ?? [], memory: true,
      permissions: draft.permissions ?? ['network'], homePath: `agents/${id}`, color: draft.color ?? '#8b5cf6',
      x: 160 + Math.random() * 160, y: 120 + Math.random() * 120,
    };
    await persistAgent(agent);
    return agent;
  }, [persistAgent]);

  return { agents, setAgents, persistAgent, deleteAgent, createAgent };
}
