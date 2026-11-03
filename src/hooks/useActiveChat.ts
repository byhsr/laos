import { create } from 'zustand';

// Which agent's chat the user is looking at. A global action (the voice hotkey)
// uses this to talk to the conversation in front of them instead of guessing.
type ActiveChatState = {
  agentId: string | null;
  setActiveAgent: (id: string | null) => void;
};

export const useActiveChat = create<ActiveChatState>((set) => ({
  agentId: null,
  setActiveAgent: (id) => set({ agentId: id }),
}));
