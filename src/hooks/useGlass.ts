import { create } from 'zustand';

// Glass-mode appearance. Like the other pure-UI prefs (pen, nav labels, sidebar
// tabs) it lives in localStorage rather than the app database, and it writes
// itself onto :root as --glass-* custom properties that index.css consumes for
// the glass theme. Nothing reads the store except the settings panel.
const KEY = 'laos.glass';

export type GlassPrefs = {
  image: string;    // a backdrop image URL/data URL; '' = the built-in backdrop
  blur: number;     // px — how heavy the frost reads
  saturate: number; // % — how much colour the frost keeps
  tint: number;     // 0..1 — how dark the panel's own tint is
};

export const GLASS_BLUR_STOPS = [
  { value: '0', label: 'clear' },
  { value: '16', label: 'soft' },
  { value: '32', label: 'frosted' },
  { value: '48', label: 'heavy' },
  { value: '64', label: 'deep' },
];

export const GLASS_TINT_STOPS = [
  { value: '0.2', label: 'sheer' },
  { value: '0.35', label: 'light' },
  { value: '0.5', label: 'balanced' },
  { value: '0.65', label: 'dense' },
  { value: '0.8', label: 'solid' },
];

export const DEFAULT_GLASS: GlassPrefs = { image: '', blur: 32, saturate: 150, tint: 0.5 };

const read = (): GlassPrefs => {
  try {
    const raw = JSON.parse(localStorage.getItem(KEY) || '{}') as Partial<GlassPrefs>;
    return {
      image: typeof raw.image === 'string' ? raw.image : DEFAULT_GLASS.image,
      blur: typeof raw.blur === 'number' ? raw.blur : DEFAULT_GLASS.blur,
      saturate: typeof raw.saturate === 'number' ? raw.saturate : DEFAULT_GLASS.saturate,
      tint: typeof raw.tint === 'number' ? raw.tint : DEFAULT_GLASS.tint,
    };
  } catch { return { ...DEFAULT_GLASS }; }
};

const write = (p: GlassPrefs) => {
  try { localStorage.setItem(KEY, JSON.stringify(p)); } catch { /* ignore */ }
};

// Push the prefs onto the document as custom properties. Only data/remote image
// strings ever reach here, but strip the few characters that could break out of
// url() anyway so a stray quote can't corrupt the backdrop.
const apply = (p: GlassPrefs) => {
  const el = document.documentElement;
  const image = p.image.replace(/["'\\\n\r]/g, '');
  el.style.setProperty('--glass-image', image ? `url("${image}")` : 'none');
  // A soft dark veil sits over a user image so chrome stays legible on a busy
  // photo; the built-in gradient backdrop needs none.
  el.style.setProperty('--glass-veil', image ? 'linear-gradient(rgb(10 12 16 / 0.34), rgb(10 12 16 / 0.46))' : 'none');
  el.style.setProperty('--glass-blur', `${Math.max(0, p.blur)}px`);
  el.style.setProperty('--glass-saturate', `${Math.max(0, p.saturate)}%`);
  el.style.setProperty('--glass-tint', String(Math.min(1, Math.max(0, p.tint))));
};

// Called once on boot so a saved look is in place before first paint.
export const initGlass = () => apply(read());

// Read an image file into a data URL, downscaled to keep a full-screen photo a
// few hundred KB — a raw camera JPEG would blow past the localStorage quota.
export const readBackgroundImage = (file: File): Promise<string> =>
  new Promise((resolve, reject) => {
    const url = URL.createObjectURL(file);
    const img = new Image();
    img.onload = () => {
      URL.revokeObjectURL(url);
      try {
        const max = 1920;
        const scale = Math.min(1, max / Math.max(img.naturalWidth, img.naturalHeight));
        const w = Math.max(1, Math.round(img.naturalWidth * scale));
        const h = Math.max(1, Math.round(img.naturalHeight * scale));
        const canvas = document.createElement('canvas');
        canvas.width = w;
        canvas.height = h;
        const ctx = canvas.getContext('2d');
        if (!ctx) { reject(new Error('canvas unavailable')); return; }
        ctx.drawImage(img, 0, 0, w, h);
        // WebP keeps alpha and stays small; every webview we ship renders it.
        resolve(canvas.toDataURL('image/webp', 0.85));
      } catch { reject(new Error('could not read image')); }
    };
    img.onerror = () => { URL.revokeObjectURL(url); reject(new Error('could not read image')); };
    img.src = url;
  });

type GlassState = GlassPrefs & {
  setImage: (image: string) => void;
  setBlur: (blur: number) => void;
  setTint: (tint: number) => void;
  reset: () => void;
};

export const useGlass = create<GlassState>((set, get) => {
  const commit = (patch: Partial<GlassPrefs>) => {
    const { image, blur, saturate, tint } = get();
    const next: GlassPrefs = { image, blur, saturate, tint, ...patch };
    write(next);
    apply(next);
    set(next);
  };
  return {
    ...read(),
    setImage: (image) => commit({ image }),
    setBlur: (blur) => commit({ blur }),
    setTint: (tint) => commit({ tint }),
    reset: () => commit({ ...DEFAULT_GLASS }),
  };
});
