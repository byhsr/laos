import { create } from 'zustand';
import type { Skill } from '../types';
import { listSkills, saveSkill as saveSkillApi, deleteSkill as deleteSkillApi } from '../runtime';

type SkillsState = {
  skills: Skill[];
  setSkills: (skills: Skill[] | ((prev: Skill[]) => Skill[])) => void;
  loadSkills: () => Promise<void>;
  saveSkill: (s: Skill) => Promise<void>;
  deleteSkill: (id: string) => Promise<void>;
};

export const useSkillsStore = create<SkillsState>((set) => ({
  skills: [],

  setSkills: (skills) => set((s) => ({ skills: typeof skills === 'function' ? skills(s.skills) : skills })),

  loadSkills: async () => {
    const stored = await listSkills().catch(() => [] as Skill[]);
    if (stored.length) set({ skills: stored });
  },

  saveSkill: async (s) => {
    await saveSkillApi(s);
    set((st) => ({ skills: st.skills.some((p) => p.id === s.id) ? st.skills.map((p) => (p.id === s.id ? s : p)) : [...st.skills, s] }));
  },

  deleteSkill: async (id) => {
    await deleteSkillApi(id);
    set((st) => ({ skills: st.skills.filter((p) => p.id !== id) }));
  },
}));
