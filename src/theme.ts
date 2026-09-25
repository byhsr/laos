// The single source of truth for the themes offered in Settings.
//
// Swatch values live here rather than in a component: a theme preview has to
// show colours that are not the active ones, so this module is the one place
// hex belongs. Components only ever consume tokens.
export type ThemeKey = 'dark' | 'light' | 'cyber';

export const THEMES: { key: ThemeKey; background: string; surface: string; border: string }[] = [
  { key: 'dark', background: '#191919', surface: '#202020', border: '#2a2a2a' },
  { key: 'light', background: '#f5f5f5', surface: '#ffffff', border: '#dddddd' },
  { key: 'cyber', background: '#0c0d10', surface: '#14161c', border: '#23262d' },
];

export const activeTheme = (): ThemeKey =>
  (document.documentElement.getAttribute('data-theme') as ThemeKey) ?? 'dark';

export const applyTheme = (key: ThemeKey) => {
  document.documentElement.setAttribute('data-theme', key);
};
