import { useCallback, useEffect, useState } from 'react';
import type { Integration } from '../types';
import { initializeStorage } from '../runtime';
import { integrations as seedIntegrations } from '../data';

export function useWorkspace() {
  const [integrations, setIntegrations] = useState<Integration[]>(seedIntegrations);

  useEffect(() => {
    initializeStorage();
  }, []);

  return { integrations, setIntegrations };
}
