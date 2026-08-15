import { useEffect, useState } from 'react';
import type { Tool } from '../types';
import { listTools, saveTool as saveToolApi, deleteTool as deleteToolApi } from '../runtime';
import { initialTools } from '../data';

export function useTools() {
  const [tools, setTools] = useState<Tool[]>(initialTools);

  useEffect(() => {
    listTools().then((stored) => {
      if (stored.length) setTools(stored);
    }).catch(() => {});
  }, []);

  const saveTool = async (t: Tool) => {
    await saveToolApi(t);
    setTools((prev) => prev.some((p) => p.id === t.id) ? prev.map((p) => (p.id === t.id ? t : p)) : [...prev, t]);
  };

  const deleteTool = async (id: string) => {
    await deleteToolApi(id);
    setTools((prev) => prev.filter((p) => p.id !== id));
  };

  return { tools, setTools, saveTool, deleteTool };
}
