import { create } from 'zustand';
import type { Project } from '../types';
import { listProjects, saveProject as saveProjectApi, deleteProject as deleteProjectApi } from '../runtime';

const ACTIVE_KEY = 'laos.activeProject';

type ProjectsState = {
  projects: Project[];
  activeId: string; // the project new sessions are filed under ('' = none)
  setActive: (id: string) => void;
  loadProjects: () => Promise<void>;
  saveProject: (p: Project) => Promise<void>;
  deleteProject: (id: string) => Promise<void>;
};

export const useProjectsStore = create<ProjectsState>((set, get) => ({
  projects: [],
  activeId: (() => { try { return localStorage.getItem(ACTIVE_KEY) ?? ''; } catch { return ''; } })(),

  setActive: (id) => {
    try { if (id) localStorage.setItem(ACTIVE_KEY, id); else localStorage.removeItem(ACTIVE_KEY); } catch { /* ignore */ }
    set({ activeId: id });
  },

  loadProjects: async () => {
    const projects = await listProjects().catch(() => [] as Project[]);
    set({ projects });
    // Drop an active id whose project no longer exists.
    if (get().activeId && !projects.some((p) => p.id === get().activeId)) set({ activeId: '' });
  },

  saveProject: async (p) => {
    const id = await saveProjectApi(p);
    const saved = { ...p, id };
    set((st) => ({ projects: st.projects.some((x) => x.id === id) ? st.projects.map((x) => (x.id === id ? saved : x)) : [saved, ...st.projects] }));
  },

  deleteProject: async (id) => {
    await deleteProjectApi(id);
    if (get().activeId === id) get().setActive('');
    set((st) => ({ projects: st.projects.filter((x) => x.id !== id) }));
  },
}));
