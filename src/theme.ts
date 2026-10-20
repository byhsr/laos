// The single source of truth for the themes offered in Settings.
//
// Swatch values live here rather than in a component: a theme preview has to
// show colours that are not the active ones, so this module is the one place
// hex belongs. Components only ever consume tokens.
import { browserSetTheme } from './runtime';

export type ThemeKey = 'dark' | 'light' | 'cyber' | 'navy' | 'glass' | 'paper';

export const THEMES: { key: ThemeKey; background: string; surface: string; border: string }[] = [
  { key: 'dark', background: '#191919', surface: '#202020', border: '#2a2a2a' },
  { key: 'light', background: '#f5f5f5', surface: '#ffffff', border: '#dddddd' },
  // Greenish terminal cyberpunk.
  { key: 'cyber', background: '#060d09', surface: '#0c1712', border: '#1b3125' },
  // The original cool navy-black, kept under its own name.
  { key: 'navy', background: '#0c0d10', surface: '#14161c', border: '#23262d' },
  // One frosted panel over a muted, defocused backdrop.
  {
    key: 'glass',
    background: 'linear-gradient(150deg, #6b6f63, #4a5566 55%, #2b2f36)',
    surface: 'rgba(20,20,20,0.55)',
    border: 'rgba(255,255,255,0.18)',
  },
  // Warm paper page on a pale blue desk.
  { key: 'paper', background: '#dcebf7', surface: '#ffffff', border: '#c7ddf0' },
];

export const activeTheme = (): ThemeKey =>
  (document.documentElement.getAttribute('data-theme') as ThemeKey) ?? 'dark';

export const applyTheme = (key: ThemeKey) => {
  document.documentElement.setAttribute('data-theme', key);
  // The in-app browser follows the app theme too, so pages render light in a
  // light UI (and dark otherwise). Best-effort — a preview has no backend.
  void browserSetTheme(key === 'light' ? 'light' : 'dark');
};

// Called once on boot so the browser matches the restored theme.
export const syncBrowserTheme = () => applyTheme(activeTheme());
