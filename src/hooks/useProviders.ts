import { create } from 'zustand';
import type { ProviderRecord } from '../types';
import { listProviders, saveProvider as saveProviderApi, deleteProvider as deleteProviderApi, type ProviderDraft } from '../runtime';

// Built-in providers (read-only) plus any custom adapter the user added. Loaded
// once at bootstrap; every provider picker reads from here so a new provider
// shows up without a reload.
type ProvidersState = {
  providers: ProviderRecord[];
  loadProviders: () => Promise<void>;
  saveProvider: (p: ProviderDraft) => Promise<void>;
  deleteProvider: (id: string) => Promise<void>;
};

export const useProvidersStore = create<ProvidersState>((set) => ({
  providers: [],

  loadProviders: async () => {
    // Built-ins always come back, so a non-empty result is safe to adopt.
    const rows = await listProviders().catch(() => [] as ProviderRecord[]);
    if (rows.length) set({ providers: rows });
  },

  saveProvider: async (p) => {
    await saveProviderApi(p);
    // The key is write-only: the renderer keeps only whether one is set.
    const stored: ProviderRecord = { ...p };
    delete (stored as ProviderDraft).apiKey;
    delete (stored as ProviderDraft).clearKey;
    stored.hasKey = p.clearKey ? false : (p.apiKey?.trim() ? true : !!p.hasKey);
    set((s) => ({
      providers: s.providers.some((x) => x.id === p.id)
        ? s.providers.map((x) => (x.id === p.id ? stored : x))
        : [...s.providers, stored],
    }));
  },

  deleteProvider: async (id) => {
    await deleteProviderApi(id);
    set((s) => ({ providers: s.providers.filter((x) => x.id !== id) }));
  },
}));
