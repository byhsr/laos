import { create } from 'zustand';
import type { Integration } from '../types';
import { initializeStorage } from '../runtime';
import { integrations as seedIntegrations } from '../data';

type WorkspaceState = {
  integrations: Integration[];
  setIntegrations: (integrations: Integration[] | ((prev: Integration[]) => Integration[])) => void;
  loadWorkspace: () => Promise<void>;
};

export const useWorkspaceStore = create<WorkspaceState>((set) => ({
  integrations: seedIntegrations,

  setIntegrations: (integrations) => set((s) => ({ integrations: typeof integrations === 'function' ? integrations(s.integrations) : integrations })),

  loadWorkspace: async () => {
    await initializeStorage().catch(() => {});
  },
}));
