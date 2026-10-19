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
  openPanel: () => void;
  closePanel: () => void;
  applyState: (s: BrowserState) => void;
};

export const useBrowserStore = create<BrowserStore>((set) => ({
  panelOpen: false,
  visible: false,
  url: '',
  title: '',
  loading: false,
  openPanel: () => set({ panelOpen: true }),
  closePanel: () => set({ panelOpen: false }),
  applyState: (s) => set((prev) => ({
    visible: s.visible,
    // Empty title/url on a plain show event shouldn't wipe what we already show.
    url: s.url || prev.url,
    title: s.title || prev.title,
    loading: s.loading,
    panelOpen: s.visible ? true : prev.panelOpen,
  })),
}));
