import { create } from 'zustand';
import type { Agent, Run, RunEvent } from '../types';
import { executeAgent, listRuns } from '../runtime';

type RunsState = {
  runs: Run[];
  setRuns: (runs: Run[] | ((prev: Run[]) => Run[])) => void;
  loadRuns: () => Promise<void>;
  handleRun: (input: string, agent: Agent) => Promise<{ output: string; events: RunEvent[] }>;
  clearRuns: () => void;
};

export const useRunsStore = create<RunsState>((set) => ({
  runs: [],

  setRuns: (runs) => set((s) => ({ runs: typeof runs === 'function' ? runs(s.runs) : runs })),

  loadRuns: async () => {
    const stored = await listRuns().catch(() => [] as Run[]);
    if (stored.length) set({ runs: stored });
  },

  handleRun: async (input, agent) => {
    const run: Run = { id: `run-${Date.now()}`, agentId: agent.id, startedAt: new Date().toISOString(), status: 'running', model: agent.model, input, events: [{ time: new Date().toLocaleTimeString(), type: 'thought', title: 'Starting run' }] };
    set((s) => ({ runs: [run, ...s.runs] }));
    try {
      const result = await executeAgent(agent, input);
      const events: RunEvent[] = result.events.length ? result.events : [{ time: new Date().toLocaleTimeString(), type: 'result', title: 'Generated final result' }];
      set((s) => ({ runs: s.runs.map((r) => (r.id === run.id ? { ...r, status: 'completed', endedAt: new Date().toISOString(), events, output: result.output, promptTokens: result.promptTokens ?? 0, completionTokens: result.completionTokens ?? 0 } : r)) }));
      return result;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      set((s) => ({ runs: s.runs.map((r) => (r.id === run.id ? { ...r, status: 'failed', endedAt: new Date().toISOString(), events: [...r.events, { time: new Date().toLocaleTimeString(), type: 'result', title: 'Run failed', detail: message }] } : r)) }));
      throw error;
    }
  },

  clearRuns: () => set({ runs: [] }),
}));
