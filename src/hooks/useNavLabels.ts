import { create } from 'zustand';

// User-renameable topbar sections. These are a per-machine UI preference, so they
// live in localStorage rather than the app database.
export type NavKey = 'home' | 'graph' | 'manager' | 'workflows' | 'tasks' | 'workshop' | 'runs' | 'telegram' | 'settings';

export const NAV_SECTIONS: { key: NavKey; label: string }[] = [
  { key: 'home', label: 'Home' },
  { key: 'graph', label: 'Graph' },
  { key: 'manager', label: 'Laos' },
  { key: 'workflows', label: 'Workflows' },
  { key: 'tasks', label: 'Tasks' },
  { key: 'workshop', label: 'Workshop' },
  { key: 'runs', label: 'Runs' },
  { key: 'telegram', label: 'Telegram' },
  { key: 'settings', label: 'Settings' },
];

const STORAGE_KEY = 'laos.navLabels';

const read = (): Record<string, string> => {
  try { return JSON.parse(localStorage.getItem(STORAGE_KEY) || '{}') as Record<string, string>; } catch { return {}; }
};

const write = (labels: Record<string, string>) => {
  try { localStorage.setItem(STORAGE_KEY, JSON.stringify(labels)); } catch { /* ignore */ }
};

type NavLabelsState = {
  labels: Record<string, string>;
  setLabel: (key: NavKey, value: string) => void;
  reset: () => void;
};

export const useNavLabels = create<NavLabelsState>((set, get) => ({
  labels: read(),

  // An empty value clears the override, restoring the default name.
  setLabel: (key, value) => {
    const labels = { ...get().labels };
    const trimmed = value.trim();
    if (trimmed) labels[key] = trimmed; else delete labels[key];
    write(labels);
    set({ labels });
  },

  reset: () => {
    try { localStorage.removeItem(STORAGE_KEY); } catch { /* ignore */ }
    set({ labels: {} });
  },
}));

// Resolves a section's display name, falling back to its default.
export const navLabel = (labels: Record<string, string>, key: NavKey): string =>
  labels[key] || (NAV_SECTIONS.find((s) => s.key === key)?.label ?? key);
