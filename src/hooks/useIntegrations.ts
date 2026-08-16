import { create } from 'zustand';
import type { Integration } from '../types';
import { completeOAuth, connectOAuth, listIntegrations, saveIntegrationConfig, testIntegration } from '../runtime';

type IntegrationsState = {
  integrations: Integration[];
  loadIntegrations: () => Promise<void>;
  saveConfig: (id: string, config: Record<string, unknown>) => Promise<void>;
  connect: (id: string) => Promise<string>;
  test: (id: string) => Promise<boolean>;
};

export const useIntegrationsStore = create<IntegrationsState>((set, get) => ({
  integrations: [],

  loadIntegrations: async () => {
    const stored = await listIntegrations().catch(() => [] as Integration[]);
    set({ integrations: stored });
  },

  saveConfig: async (id, config) => {
    await saveIntegrationConfig(id, config);
    await get().loadIntegrations();
  },

  connect: async (id) => {
    // OAuth integrations open the provider flow in the system browser; the
    // backend listens on loopback in the background and exchanges the code.
    // Returns the auth URL for user feedback.
    const oauthProviders = ['sheets', 'docs', 'notion'];
    if (oauthProviders.includes(id)) {
      const url = await connectOAuth(id);
      return url;
    }
    return '';
  },

  test: async (id) => testIntegration(id),
}));
