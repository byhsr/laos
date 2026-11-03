import { create } from 'zustand';
import {
  deleteResource, listAdapters, listConnections, listResources, saveResource,
  type Adapter, type Connection, type Resource,
} from '../runtime';

// The resource/connection/adapter view's data: the adapter catalog, the
// connections the app holds (integrations + LLM endpoints), and the resources
// those connections expose.
type State = {
  adapters: Adapter[];
  connections: Connection[];
  resources: Resource[];
  load: () => Promise<void>;
  addResource: (r: Resource) => Promise<void>;
  removeResource: (id: string) => Promise<void>;
};

export const useConnectionsStore = create<State>((set, get) => ({
  adapters: [],
  connections: [],
  resources: [],
  load: async () => {
    const [adapters, connections, resources] = await Promise.all([listAdapters(), listConnections(), listResources()]);
    set({ adapters, connections, resources });
  },
  addResource: async (r) => { await saveResource(r); await get().load(); },
  removeResource: async (id) => { await deleteResource(id); await get().load(); },
}));
