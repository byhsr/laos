import { create } from 'zustand';
import type { CreateKind } from '../types';

// What the topbar "+" creates. A per-machine UI preference, so it lives in
// localStorage rather than the app database (same as the nav labels).
export const CREATE_KINDS: { kind: CreateKind; label: string }[] = [
  { kind: 'agent', label: 'agent' },
  { kind: 'workflow', label: 'workflow' },
  { kind: 'skill', label: 'skill' },
  { kind: 'script', label: 'app' },
  { kind: 'project', label: 'project' },
  { kind: 'tool', label: 'tool' },
  { kind: 'model', label: 'model' },
];

const STORAGE_KEY = 'laos.createDefault';

const isKind = (v: unknown): v is CreateKind => CREATE_KINDS.some((c) => c.kind === v);

const read = (): CreateKind => {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return isKind(raw) ? raw : 'agent';
  } catch {
    return 'agent';
  }
};

type CreateDefaultState = {
  kind: CreateKind;
  setKind: (kind: CreateKind) => void;
};

export const useCreateDefault = create<CreateDefaultState>((set) => ({
  kind: read(),
  setKind: (kind) => {
    try { localStorage.setItem(STORAGE_KEY, kind); } catch { /* ignore */ }
    set({ kind });
  },
}));

export const createLabel = (kind: CreateKind): string =>
  CREATE_KINDS.find((c) => c.kind === kind)?.label ?? kind;
