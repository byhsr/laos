import { create } from 'zustand';
import { initializeStorage } from '../runtime';

type WorkspaceState = {
  loadWorkspace: () => Promise<void>;
};

export const useWorkspaceStore = create<WorkspaceState>(() => ({
  loadWorkspace: async () => {
    await initializeStorage().catch(() => {});
  },
}));
