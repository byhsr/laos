import { create } from 'zustand';
import type { Script } from '../types';
import { listScripts, saveScript as saveScriptApi, deleteScript as deleteScriptApi } from '../runtime';

type ScriptsState = {
  scripts: Script[];
  setScripts: (scripts: Script[] | ((prev: Script[]) => Script[])) => void;
  loadScripts: () => Promise<void>;
  saveScript: (s: Script) => Promise<void>;
  deleteScript: (id: string) => Promise<void>;
};

export const useScriptsStore = create<ScriptsState>((set) => ({
  scripts: [],

  setScripts: (scripts) => set((s) => ({ scripts: typeof scripts === 'function' ? scripts(s.scripts) : scripts })),

  loadScripts: async () => {
    const stored = await listScripts().catch(() => [] as Script[]);
    set({ scripts: stored });
  },

  saveScript: async (s) => {
    const id = await saveScriptApi(s);
    const saved = { ...s, id };
    set((st) => ({ scripts: st.scripts.some((p) => p.id === saved.id) ? st.scripts.map((p) => (p.id === saved.id ? saved : p)) : [...st.scripts, saved] }));
  },

  deleteScript: async (id) => {
    await deleteScriptApi(id);
    set((st) => ({ scripts: st.scripts.filter((p) => p.id !== id) }));
  },
}));
