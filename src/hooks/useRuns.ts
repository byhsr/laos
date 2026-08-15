import { useCallback, useState } from 'react';
import type { Agent, Run, RunEvent } from '../types';
import { executeAgent } from '../runtime';

export function useRuns() {
  const [runs, setRuns] = useState<Run[]>([]);

  const handleRun = useCallback(async (input: string, agent: Agent): Promise<{ output: string; events: RunEvent[] }> => {
    const run: Run = { id: `run-${Date.now()}`, agentId: agent.id, startedAt: new Date().toISOString(), status: 'running', model: agent.model, input, events: [{ time: new Date().toLocaleTimeString(), type: 'thought', title: 'Starting run' }] };
    setRuns((prev) => [run, ...prev]);
    try {
      const result = await executeAgent(agent, input);
      const events: RunEvent[] = result.events.length ? result.events : [{ time: new Date().toLocaleTimeString(), type: 'result', title: 'Generated final result' }];
      setRuns((prev) => prev.map((r) => (r.id === run.id ? { ...r, status: 'completed', endedAt: new Date().toISOString(), events, output: result.output, promptTokens: result.promptTokens ?? 0, completionTokens: result.completionTokens ?? 0 } : r)));
      return result;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setRuns((prev) => prev.map((r) => (r.id === run.id ? { ...r, status: 'failed', endedAt: new Date().toISOString(), events: [...r.events, { time: new Date().toLocaleTimeString(), type: 'result', title: 'Run failed', detail: message }] } : r)));
      throw error;
    }
  }, []);

  const clearRuns = useCallback(() => setRuns([]), []);

  return { runs, setRuns, handleRun, clearRuns };
}
