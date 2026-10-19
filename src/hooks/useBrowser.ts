import { create } from 'zustand';
import type { BrowserState } from '../runtime';

// The mini-browser drawer. `panelOpen` is the UI's own state (so the user can
// close/reopen it); `visible/url/title/loading` mirror the backend webview, from
// the `lup-browser` events. A browser action from an agent slides the panel open.
type BrowserStore = {
  panelOpen: boolean;
  visible: boolean;
  url: string;
  title: string;
  loading: boolean;
  width: number;
  openPanel: () => void;
  closePanel: () => void;
  setWidth: (w: number) => void;
  applyState: (s: BrowserState) => void;
};

const WIDTH_KEY = 'laos.browserWidth';
const MIN_WIDTH = 300;
const DEFAULT_WIDTH = 420;

// Width is remembered across sessions so the panel opens where you left it.
const loadWidth = (): number => {
  try {
    const raw = Number(localStorage.getItem(WIDTH_KEY));
    return Number.isFinite(raw) && raw >= MIN_WIDTH ? raw : DEFAULT_WIDTH;
  } catch {
    return DEFAULT_WIDTH;
  }
};

export const useBrowserStore = create<BrowserStore>((set) => ({
  panelOpen: false,
  visible: false,
  url: '',
  title: '',
  loading: false,
  width: loadWidth(),
  openPanel: () => set({ panelOpen: true }),
  closePanel: () => set({ panelOpen: false }),
  setWidth: (w) => {
    try { localStorage.setItem(WIDTH_KEY, String(w)); } catch { /* ignore */ }
    set({ width: w });
  },
  applyState: (s) => set((prev) => ({
    visible: s.visible,
    // Empty title/url on a plain show event shouldn't wipe what we already show.
    url: s.url || prev.url,
    title: s.title || prev.title,
    loading: s.loading,
    panelOpen: s.visible ? true : prev.panelOpen,
  })),
}));
