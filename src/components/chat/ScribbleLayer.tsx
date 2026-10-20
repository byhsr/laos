import { useCallback, useEffect, useRef } from 'react';
import { useScribble, useScribbleDrawing } from '../../hooks/useScribble';
import { useThemeKey } from '../../hooks/useTheme';
import { penColor } from '../../pen';

// A scribble surface over a chat. Paper theme only: the whole feature is inert
// in every other theme (no canvas, no listeners, nothing persisted).
//
// The canvas is a transparent overlay filling the chat area, sitting BELOW the
// message list and the composer at rest. While drawing it lifts above the
// message list — a layer underneath the list would never receive a pointerdown,
// because the list swallows the event.
//
// Strokes are stored in content coordinates (viewport y + the list's scrollTop)
// and re-offset at paint time, so a pinned note stays anchored to the message it
// was written beside.
const FADE_MS = 9000; // default strokes retire after ~9s
const BASE_WIDTH = 2;
const OPACITY = 0.7;

type Pt = { x: number; y: number; w: number };
type Stroke = { pts: Pt[]; keep: boolean; bornAt: number; color: string };

const keyOf = (id: string) => `scribble:${id}`;
const prefersReduced = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches;

export function ScribbleLayer({ storeKey, scrollRef }: {
  storeKey: string;
  scrollRef: { current: HTMLDivElement | null };
}) {
  const enabled = useThemeKey() === 'paper';
  const drawing = useScribbleDrawing();
  const pinned = useScribble((s) => s.pinned);
  const pen = useScribble((s) => s.pen);
  const clearToken = useScribble((s) => s.clearToken);
  const setAlt = useScribble((s) => s.setAlt);

  const canvasRef = useRef<HTMLCanvasElement>(null);
  const strokes = useRef<Stroke[]>([]);
  const cur = useRef<Stroke | null>(null);
  const active = useRef(false);
  const size = useRef({ w: 0, h: 0 });
  const raf = useRef(0);
  const seenClear = useRef(clearToken);
  const pinnedRef = useRef(pinned);
  pinnedRef.current = pinned;
  // Paint must never read `enabled` through a stale closure: a frame queued
  // before a theme switch would otherwise keep drawing with enabled: false.
  const enabledRef = useRef(enabled);
  enabledRef.current = enabled;

  // Measure from the element's rendered box. The stored size is never trusted:
  // a canvas that was measured while its view was hidden reads 0×0, and a stale
  // zero would make every stroke invisible. Re-deriving here is cheap and
  // self-heals as soon as anything paints.
  const sync = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return false;
    const dpr = window.devicePixelRatio || 1;
    const r = canvas.getBoundingClientRect();
    const w = Math.max(1, Math.round(r.width));
    const h = Math.max(1, Math.round(r.height));
    const bw = Math.round(w * dpr);
    const bh = Math.round(h * dpr);
    if (canvas.width !== bw || canvas.height !== bh) {
      canvas.width = bw;
      canvas.height = bh;
      canvas.getContext('2d')?.setTransform(dpr, 0, 0, dpr, 0, 0);
    }
    size.current = { w, h };
    return true;
  }, []);

  const paint = useCallback(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext('2d');
    if (!canvas || !ctx || !sync()) return;
    const { w, h } = size.current;
    ctx.clearRect(0, 0, w, h); // clearRect only — the canvas is never filled
    if (!enabledRef.current) return;
    const scroll = scrollRef.current?.scrollTop ?? 0;
    const now = performance.now();
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';
    const list = cur.current ? [...strokes.current, cur.current] : strokes.current;
    for (const s of list) {
      if (s.pts.length === 0) continue;
      ctx.globalAlpha = s.keep ? OPACITY : OPACITY * Math.max(0, 1 - (now - s.bornAt) / FADE_MS);
      if (ctx.globalAlpha <= 0) continue;
      ctx.strokeStyle = s.color;
      ctx.fillStyle = s.color;
      if (s.pts.length === 1) {
        const p = s.pts[0];
        ctx.beginPath();
        ctx.arc(p.x, p.y - scroll, (BASE_WIDTH * p.w) / 2, 0, Math.PI * 2);
        ctx.fill();
        continue;
      }
      const avgW = s.pts.reduce((n, p) => n + p.w, 0) / s.pts.length;
      ctx.lineWidth = BASE_WIDTH * Math.min(1.4, Math.max(0.6, avgW));
      ctx.beginPath();
      ctx.moveTo(s.pts[0].x, s.pts[0].y - scroll);
      // Quadratic through the midpoints — a smoothed, hand-drawn line.
      for (let i = 1; i < s.pts.length - 1; i++) {
        const a = s.pts[i];
        const b = s.pts[i + 1];
        ctx.quadraticCurveTo(a.x, a.y - scroll, (a.x + b.x) / 2, (a.y + b.y) / 2 - scroll);
      }
      const last = s.pts[s.pts.length - 1];
      ctx.lineTo(last.x, last.y - scroll);
      ctx.stroke();
    }
    ctx.globalAlpha = 1;
  }, [sync, scrollRef]);

  const frame = useCallback((t: number) => {
    if (!pinnedRef.current && !prefersReduced()) {
      strokes.current = strokes.current.filter((s) => s.keep || t - s.bornAt < FADE_MS);
    }
    paint();
    const alive = !pinnedRef.current && !prefersReduced() && strokes.current.some((s) => !s.keep);
    raf.current = active.current || alive ? requestAnimationFrame(frame) : 0;
  }, [paint]);

  const kick = useCallback(() => {
    if (!raf.current) raf.current = requestAnimationFrame(frame);
  }, [frame]);

  const clear = useCallback(() => {
    strokes.current = [];
    cur.current = null;
    try { localStorage.removeItem(keyOf(storeKey)); } catch { /* ignore */ }
    paint();
  }, [storeKey, paint]);

  // Reset on theme/chat change; load anything pinned for this chat.
  useEffect(() => {
    if (raf.current) cancelAnimationFrame(raf.current);
    raf.current = 0;
    active.current = false;
    cur.current = null;
    strokes.current = [];
    if (enabled) {
      try {
        const raw = localStorage.getItem(keyOf(storeKey));
        const saved = raw ? (JSON.parse(raw) as { pts: Pt[]; color?: string }[]) : [];
        const fallback = penColor(useScribble.getState().pen);
        strokes.current = saved.map((s) => ({ pts: s.pts, keep: true, bornAt: 0, color: s.color ?? fallback }));
      } catch { strokes.current = []; }
    }
    paint();
  }, [enabled, storeKey, paint]);

  // The topbar's clear button bumps a token; every layer answers it once.
  useEffect(() => {
    if (clearToken === seenClear.current) return;
    seenClear.current = clearToken;
    clear();
  }, [clearToken, clear]);

  // Repaint whenever the box changes — and once on mount, in case the view was
  // hidden (0×0) when it was first measured.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !enabled) return;
    const ro = new ResizeObserver(() => paint());
    ro.observe(canvas);
    paint();
    return () => ro.disconnect();
  }, [enabled, paint]);

  // Repaint on scroll so strokes stay pinned to the content.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el || !enabled) return;
    const onScroll = () => paint();
    el.addEventListener('scroll', onScroll, { passive: true });
    return () => el.removeEventListener('scroll', onScroll);
  }, [enabled, scrollRef, paint]);

  // Hold Alt to draw without the toggle.
  useEffect(() => {
    if (!enabled) return;
    const kd = (e: KeyboardEvent) => { if (e.key === 'Alt') setAlt(true); };
    const ku = (e: KeyboardEvent) => { if (e.key === 'Alt') setAlt(false); };
    const blur = () => setAlt(false);
    window.addEventListener('keydown', kd);
    window.addEventListener('keyup', ku);
    window.addEventListener('blur', blur);
    return () => {
      window.removeEventListener('keydown', kd);
      window.removeEventListener('keyup', ku);
      window.removeEventListener('blur', blur);
    };
  }, [enabled, setAlt]);

  if (!enabled) return null;

  const persist = () => {
    try {
      localStorage.setItem(keyOf(storeKey), JSON.stringify(strokes.current.filter((s) => s.keep)));
    } catch { /* ignore */ }
  };

  // Coordinates come from the canvas's own rect, never from window coordinates.
  const pos = (e: React.PointerEvent<HTMLCanvasElement>): Pt => {
    const r = e.currentTarget.getBoundingClientRect();
    return {
      x: e.clientX - r.left,
      y: e.clientY - r.top + (scrollRef.current?.scrollTop ?? 0),
      w: e.pressure > 0 ? 0.4 + e.pressure : 1,
    };
  };

  const down = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (!drawing) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    active.current = true;
    cur.current = { pts: [pos(e)], keep: pinned || prefersReduced(), bornAt: performance.now(), color: penColor(pen) };
    kick();
  };

  const move = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (!active.current || !cur.current) return;
    cur.current.pts.push(pos(e));
    kick();
  };

  const up = () => {
    if (!active.current) return;
    active.current = false;
    const s = cur.current;
    cur.current = null;
    if (s && s.pts.length > 0) {
      strokes.current.push(s);
      if (s.keep) persist();
    }
    kick();
  };

  // While drawing, the canvas sits above the list, so the wheel lands on a
  // non-scrollable element and the list would never move. Forward it by hand.
  const wheel = (e: React.WheelEvent<HTMLCanvasElement>) => {
    const el = scrollRef.current;
    if (!el) return;
    const unit = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? el.clientHeight : 1;
    el.scrollTop += e.deltaY * unit;
    el.scrollLeft += e.deltaX * unit;
  };

  return (
    <canvas
      ref={canvasRef}
      className="scribble-canvas absolute inset-0"
      style={{
        // A canvas is a replaced element: `inset: 0` alone leaves it at its
        // intrinsic 300×150. The explicit 100% is what actually fills the chat
        // area — without it the surface is a small box in the corner.
        width: '100%',
        height: '100%',
        pointerEvents: drawing ? 'auto' : 'none',
        touchAction: drawing ? 'none' : 'auto',
        userSelect: 'none',
        zIndex: drawing ? 20 : 0,
        cursor: drawing ? 'var(--cursor-action)' : 'inherit',
      }}
      onPointerDown={down}
      onPointerMove={move}
      onPointerUp={up}
      onPointerCancel={up}
      onPointerLeave={up}
      onWheel={wheel}
      onDoubleClick={clear}
    />
  );
}
