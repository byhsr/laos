import { create } from 'zustand';
import type { ModelConfig } from '../types';
import { listModels, saveModel as saveModelApi, deleteModel as deleteModelApi } from '../runtime';
import { useProvidersStore } from './useProviders';
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
    // A model has no key of its own — whether it can authenticate is a property of
    // its provider, so mirror the provider's key state rather than the row's.
    const hasKey = useProvidersStore.getState().providers.find((p) => p.id === m.provider)?.hasKey ?? !!m.hasKey;
    const stored: ModelConfig = { ...m, hasKey };
    set((s) => ({ models: s.models.some((p) => p.id === m.id) ? s.models.map((p) => (p.id === m.id ? stored : p)) : [...s.models, stored] }));
  },

  deleteModel: async (id) => {
    await deleteModelApi(id);
    set((s) => ({ models: s.models.filter((p) => p.id !== id) }));
  },
}));
