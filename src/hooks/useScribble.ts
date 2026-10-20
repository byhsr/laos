import { create } from 'zustand';
import { DEFAULT_PEN } from '../pen';

// Scribble mode is shared between the topbar controls and the per-chat layers,
// so it lives in a store rather than in either component. The layers read
// `active`/`alt`/`pinned`/`pen`; `clear()` bumps a token every layer watches.
const PEN_KEY = 'laos.pen';

const readPen = (): string => {
  try { return localStorage.getItem(PEN_KEY) || DEFAULT_PEN; } catch { return DEFAULT_PEN; }
};

const writePen = (pen: string) => {
  try { localStorage.setItem(PEN_KEY, pen); } catch { /* ignore */ }
};

type ScribbleState = {
  active: boolean;
  alt: boolean;
  pinned: boolean;
  pen: string;
  clearToken: number;
  toggleActive: () => void;
  setAlt: (on: boolean) => void;
  togglePinned: () => void;
  setPen: (pen: string) => void;
  clear: () => void;
};

export const useScribble = create<ScribbleState>((set) => ({
  active: false,
  alt: false,
  pinned: false,
  pen: readPen(),
  clearToken: 0,
  toggleActive: () => set((s) => ({ active: !s.active })),
  setAlt: (on) => set({ alt: on }),
  togglePinned: () => set((s) => ({ pinned: !s.pinned })),
  setPen: (pen) => {
    writePen(pen);
    set({ pen });
  },
  clear: () => set((s) => ({ clearToken: s.clearToken + 1 })),
}));

// Drawing is live while the pencil is toggled or Alt is held.
export const useScribbleDrawing = () => useScribble((s) => s.active || s.alt);
