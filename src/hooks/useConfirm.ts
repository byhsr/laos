import { create } from 'zustand';
import type { ManagerConfirmRequest } from '../runtime';
import { confirmManagerTool } from '../runtime';

export type PendingConfirm = ManagerConfirmRequest & { resolve: (result: string | null) => void };

type ConfirmState = {
  pending: PendingConfirm | null;
  request: (r: ManagerConfirmRequest) => Promise<string | null>;
  resolve: (approved: boolean, editedArgs: Record<string, unknown>) => void;
};

export const useConfirmStore = create<ConfirmState>((set, get) => ({
  pending: null,

  request: (r) => new Promise<string | null>((resolve) => {
    set({ pending: { ...r, resolve } });
  }),

  resolve: async (approved, editedArgs) => {
    const p = get().pending;
    if (!p) return;
    set({ pending: null });
    const result = await confirmManagerTool(p.requestId, approved, p.tool, editedArgs);
    p.resolve(result);
  },
}));
