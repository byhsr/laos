import { create } from 'zustand';
import type { ThemeKey } from '../theme';

// Per-theme colour overrides. Each token maps straight onto a --color-* custom
// property, so an override is just an inline :root value that beats the theme's
// own rule while that theme is active — and is removed the moment you switch
// away, so paper's tweaks never leak into glass. Presentation only, so like the
// other pure-UI prefs it lives in localStorage rather than the database.
const KEY = 'laos.theme.colors';

// The tokens a theme is actually built from. `foreground` is the app's `primary`
// (--color-primary aliases it), which is why it is labelled "text" here.
export type ColorToken = 'background' | 'surface' | 'border' | 'foreground' | 'muted' | 'accent';

export const COLOR_TOKENS: { key: ColorToken; label: string }[] = [
  { key: 'background', label: 'background' },
  { key: 'surface', label: 'surface' },
  { key: 'border', label: 'border' },
  { key: 'foreground', label: 'text' },
  { key: 'muted', label: 'muted' },
  { key: 'accent', label: 'accent' },
];

// The preset ramp offered in the picker — neutrals, the built-in theme planes,
// and the desaturated accents the app already uses.
export const COLOR_PRESETS = [
  '#f0efed', '#c8c8c8', '#8a8a8a', '#5a5a5a', '#3a3a3a', '#2a2a2a', '#202020', '#191919',
  '#ffffff', '#f5f5f5', '#dddddd', '#dcebf7', '#fbfaf7', '#e6e1d7', '#14161c', '#0c0d10',
  '#d7ff38', '#8fb4dc', '#7d9ec4', '#7dae85', '#c07f7f', '#b09a70', '#e08a7f', '#7fbdc0',
];

export type ThemeColorOverrides = Partial<Record<ThemeKey, Partial<Record<ColorToken, string>>>>;

const read = (): ThemeColorOverrides => {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(KEY) || '{}');
    return raw && typeof raw === 'object' ? (raw as ThemeColorOverrides) : {};
  } catch { return {}; }
};

const write = (o: ThemeColorOverrides) => {
  try { localStorage.setItem(KEY, JSON.stringify(o)); } catch { /* ignore */ }
};

const activeThemeKey = (): ThemeKey =>
  (document.documentElement.getAttribute('data-theme') as ThemeKey) ?? 'dark';

// In glass the planes are translucent tints over the backdrop, so an override
// retints their colour *channels* and the alpha — the frost itself — is left
// untouched. Everything else, and every other theme, sets the token outright.
const GLASS_CHANNEL_VAR: Partial<Record<ColorToken, string>> = {
  background: '--glass-background-rgb',
  surface: '--glass-surface-rgb',
  border: '--glass-border-rgb',
};

const hexToChannels = (hex: string): string | null => {
  const m = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return null;
  const h = m[1].length === 3 ? m[1].split('').map((c) => c + c).join('') : m[1];
  const n = parseInt(h, 16);
  return `${(n >> 16) & 255} ${(n >> 8) & 255} ${n & 255}`;
};

// Paint the active theme's overrides onto :root, clearing any token the user has
// not overridden so the theme's own rule shows through.
export const applyThemeColors = () => {
  const el = document.documentElement;
  const theme = activeThemeKey();
  const ov = useThemeColors.getState().overrides[theme] ?? {};
  for (const t of COLOR_TOKENS) {
    const value = ov[t.key];
    const channelVar = theme === 'glass' ? GLASS_CHANNEL_VAR[t.key] : undefined;
    if (channelVar) {
      const channels = value ? hexToChannels(value) : null;
      if (channels) el.style.setProperty(channelVar, channels);
      else el.style.removeProperty(channelVar);
      // Fall back to the raw token only if the value isn't a colour we can split.
      if (value && !channels) el.style.setProperty(`--color-${t.key}`, value);
      else el.style.removeProperty(`--color-${t.key}`);
    } else {
      if (value) el.style.setProperty(`--color-${t.key}`, value);
      else el.style.removeProperty(`--color-${t.key}`);
      const stale = GLASS_CHANNEL_VAR[t.key];
      if (stale) el.style.removeProperty(stale);
    }
  }
};

type State = {
  overrides: ThemeColorOverrides;
  setColor: (theme: ThemeKey, token: ColorToken, value: string) => void;
  resetTheme: (theme: ThemeKey) => void;
};

export const useThemeColors = create<State>((set, get) => ({
  overrides: read(),
  setColor: (theme, token, value) => {
    const next = { ...(get().overrides[theme] ?? {}) };
    if (value) next[token] = value; else delete next[token];
    const overrides = { ...get().overrides };
    if (Object.keys(next).length) overrides[theme] = next; else delete overrides[theme];
    write(overrides);
    set({ overrides });
    applyThemeColors();
  },
  resetTheme: (theme) => {
    const overrides = { ...get().overrides };
    delete overrides[theme];
    write(overrides);
    set({ overrides });
    applyThemeColors();
  },
}));

// Called once on boot, after the theme is restored, and re-paints whenever the
// active theme changes so the right overrides are always the ones in force.
export const initThemeColors = () => {
  applyThemeColors();
  new MutationObserver(applyThemeColors).observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['data-theme'],
  });
};
