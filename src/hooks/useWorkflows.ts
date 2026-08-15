import { create } from 'zustand';
import type { Workflow, WorkflowRunResult } from '../types';
import { deleteWorkflow as deleteWorkflowApi, executeWorkflow, listWorkflows, saveWorkflow as saveWorkflowApi } from '../runtime';

export const newWorkflow = (): Workflow => ({ id: `wf-${Date.now()}`, name: 'Untitled workflow', nodes: [], edges: [], updatedAt: new Date().toISOString() });

type WorkflowsState = {
  workflows: Workflow[];
  setWorkflows: (workflows: Workflow[] | ((prev: Workflow[]) => Workflow[])) => void;
  loadWorkflows: () => Promise<void>;
  saveWorkflow: (wf: Workflow) => Promise<Workflow>;
  deleteWorkflow: (id: string) => Promise<void>;
  runWorkflow: (wf: Workflow, input: string) => Promise<WorkflowRunResult>;
};

export const useWorkflowsStore = create<WorkflowsState>((set) => ({
  workflows: [],

  setWorkflows: (workflows) => set((s) => ({ workflows: typeof workflows === 'function' ? workflows(s.workflows) : workflows })),

  loadWorkflows: async () => {
    const stored = await listWorkflows().catch(() => [] as Workflow[]);
    set({ workflows: stored });
  },

  saveWorkflow: async (wf) => {
    const updated = { ...wf, updatedAt: new Date().toISOString() };
    set((s) => ({ workflows: s.workflows.some((w) => w.id === wf.id) ? s.workflows.map((w) => (w.id === wf.id ? updated : w)) : [updated, ...s.workflows] }));
    try { await saveWorkflowApi(updated); } catch { /* browser preview */ }
    return updated;
  },

  deleteWorkflow: async (id) => {
    set((s) => ({ workflows: s.workflows.filter((w) => w.id !== id) }));
    try { await deleteWorkflowApi(id); } catch { /* browser preview */ }
  },

  runWorkflow: async (wf, input) => {
    try {
      return await executeWorkflow(wf, input);
    } catch (error) {
      if ('__TAURI_INTERNALS__' in window) throw error;
      throw new Error('Workflow runs require the desktop app. Run `npm.cmd run tauri dev`.');
    }
  },
}));
