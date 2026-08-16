import { create } from 'zustand';
import type { Task } from '../types';
import { cancelTask as cancelTaskApi, listTasks, runTask as runTaskApi } from '../runtime';

type TasksState = {
  tasks: Task[];
  loadTasks: () => Promise<void>;
  runTask: (requester: string, assignedAgent: string, input: string, context?: string) => Promise<Task>;
  cancelTask: (id: string) => Promise<void>;
};

export const useTasksStore = create<TasksState>((set, get) => ({
  tasks: [],

  loadTasks: async () => {
    const stored = await listTasks().catch(() => [] as Task[]);
    set({ tasks: stored });
  },

  runTask: async (requester, assignedAgent, input, context = '') => {
    const task = await runTaskApi(requester, assignedAgent, input, context);
    await get().loadTasks();
    return task;
  },

  cancelTask: async (id) => {
    await cancelTaskApi(id);
    await get().loadTasks();
  },
}));
