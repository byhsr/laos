import { create } from 'zustand';
import type { NavKey } from './useNavLabels';

// Which rail sections are hidden. A per-machine UI preference, so it lives in
// localStorage rather than the app database (same as the nav labels).
const STORAGE_KEY = 'laos.hiddenTabs';

const read = (): NavKey[] => {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) || '[]');
    return Array.isArray(raw) ? raw.filter((k): k is NavKey => typeof k === 'string') : [];
  } catch {
    return [];
  }
};

const write = (hidden: NavKey[]) => {
  try { localStorage.setItem(STORAGE_KEY, JSON.stringify(hidden)); } catch { /* ignore */ }
};

type SidebarTabsState = {
  hidden: NavKey[];
  toggle: (key: NavKey) => void;
  showAll: () => void;
};

export const useSidebarTabs = create<SidebarTabsState>((set, get) => ({
  hidden: read(),

  toggle: (key) => {
    const hidden = get().hidden.includes(key)
      ? get().hidden.filter((k) => k !== key)
      : [...get().hidden, key];
    write(hidden);
    set({ hidden });
  },

  showAll: () => {
    write([]);
    set({ hidden: [] });
  },
}));
