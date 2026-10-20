import { useEffect, useState } from 'react';

// The active `data-theme`, kept in sync with the attribute Settings writes.
export const useThemeKey = () => {
  const [key, setKey] = useState(() => document.documentElement.getAttribute('data-theme') ?? 'dark');
  useEffect(() => {
    const el = document.documentElement;
    const obs = new MutationObserver(() => setKey(el.getAttribute('data-theme') ?? 'dark'));
    obs.observe(el, { attributes: true, attributeFilter: ['data-theme'] });
    return () => obs.disconnect();
  }, []);
  return key;
};
