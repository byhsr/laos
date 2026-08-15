import { create } from 'zustand';
import type { ModelConfig } from '../types';
import { listModels, saveModel as saveModelApi, deleteModel as deleteModelApi } from '../runtime';
import { models as seedModels } from '../data';

type ModelsState = {
  models: ModelConfig[];
  setModels: (models: ModelConfig[] | ((prev: ModelConfig[]) => ModelConfig[])) => void;
  loadModels: () => Promise<void>;
  saveModel: (m: ModelConfig) => Promise<void>;
  deleteModel: (id: string) => Promise<void>;
};

export const useModelsStore = create<ModelsState>((set) => ({
  models: seedModels,

  setModels: (models) => set((s) => ({ models: typeof models === 'function' ? models(s.models) : models })),

  loadModels: async () => {
    const stored = await listModels().catch(() => [] as ModelConfig[]);
    if (stored.length) set({ models: stored });
  },

  saveModel: async (m) => {
    await saveModelApi(m);
    set((s) => ({ models: s.models.some((p) => p.id === m.id) ? s.models.map((p) => (p.id === m.id ? m : p)) : [...s.models, m] }));
  },

  deleteModel: async (id) => {
    await deleteModelApi(id);
    set((s) => ({ models: s.models.filter((p) => p.id !== id) }));
  },
}));
