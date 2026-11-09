import { create } from 'zustand';

// The agent being dragged into the split row right now (its id), or null. A store
// rather than a dataTransfer payload: the window-level dragover/drop swallow and
// WebView2 event timing make dataTransfer unreliable, and both the drag sources
// (the sidebar rows, the agents grid cards) and the drop target (the pane row)
// need to read it.
type AgentDragState = {
  id: string | null;
  start: (id: string) => void;
  end: () => void;
};

export const useAgentDrag = create<AgentDragState>((set) => ({
  id: null,
  start: (id) => set({ id }),
  end: () => set({ id: null }),
}));
