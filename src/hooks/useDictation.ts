import { create } from 'zustand';
import type { DictationModel } from '../types';
import { dictationAvailable, dictationDeleteModel, dictationDownloadModel, dictationModels, dictationStart, dictationStop } from '../runtime';

// The chosen speech model is a per-machine preference — a 142 MB binary either
// exists on this disk or it does not — so it lives in localStorage rather than
// the app database, alongside the navigation labels.
const STORAGE_KEY = 'laos.dictation';
const DEFAULT_MODEL = 'base.en';

const readModel = (): string => {
  try { return localStorage.getItem(STORAGE_KEY) || DEFAULT_MODEL; } catch { return DEFAULT_MODEL; }
};

export type DictationProgress = { received: number; total: number };

type DictationState = {
  modelId: string;
  models: DictationModel[];
  available: boolean;
  recording: boolean;
  transcribing: boolean;
  downloading: DictationProgress | null;
  error?: string;
  loadModels: () => Promise<void>;
  setModelId: (id: string) => void;
  download: (id: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
  start: () => Promise<void>;
  stop: () => Promise<string>;
};

export const useDictation = create<DictationState>((set, get) => ({
  modelId: readModel(),
  models: [],
  available: true,
  recording: false,
  transcribing: false,
  downloading: null,
  error: undefined,

  loadModels: async () => {
    const [models, available] = await Promise.all([dictationModels(), dictationAvailable()]);
    set({ models, available });
  },

  setModelId: (id) => {
    try { localStorage.setItem(STORAGE_KEY, id); } catch { /* ignore */ }
    set({ modelId: id });
  },

  download: async (id) => {
    set({ downloading: { received: 0, total: 0 }, error: undefined });
    try {
      await dictationDownloadModel(id, (p) => set({ downloading: p }));
    } catch (e) {
      const message = typeof e === 'string' ? e : 'The model download failed.';
      set({ downloading: null, error: message });
      throw new Error(message);
    }
    set({ downloading: null });
    await get().loadModels();
  },

  remove: async (id) => {
    await dictationDeleteModel(id);
    await get().loadModels();
  },

  start: async () => {
    set({ error: undefined });
    try {
      await dictationStart(get().modelId);
    } catch (e) {
      const message = typeof e === 'string' ? e : 'Could not start recording.';
      set({ error: message });
      throw new Error(message);
    }
    set({ recording: true });
  },

  // Returns the transcript; the caller decides where it goes. Dictation never
  // sends a message on its own.
  stop: async () => {
    set({ recording: false, transcribing: true });
    try {
      return await dictationStop();
    } finally {
      set({ transcribing: false });
    }
  },
}));
