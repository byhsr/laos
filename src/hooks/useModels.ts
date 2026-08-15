import { useEffect, useState } from 'react';
import type { ModelConfig } from '../types';
import { listModels, saveModel as saveModelApi, deleteModel as deleteModelApi } from '../runtime';
import { models as seedModels } from '../data';

export function useModels() {
  const [models, setModels] = useState<ModelConfig[]>(seedModels);

  useEffect(() => {
    listModels().then((stored) => {
      if (stored.length) setModels(stored);
    }).catch(() => {});
  }, []);

  const saveModel = async (m: ModelConfig) => {
    await saveModelApi(m);
    setModels((prev) => prev.some((p) => p.id === m.id) ? prev.map((p) => (p.id === m.id ? m : p)) : [...prev, m]);
  };

  const deleteModel = async (id: string) => {
    await deleteModelApi(id);
    setModels((prev) => prev.filter((p) => p.id !== id));
  };

  return { models, setModels, saveModel, deleteModel };
}
